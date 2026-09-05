//! PipeWire capture of one screencast stream (a portal-negotiated video node).
//! Runs its own PipeWire main loop on a thread and hands the newest frame to
//! the render thread over a channel — DMA-BUF (dup'd fds, zero-copy) when the
//! compositor agrees to one of our formats, else a CPU copy (SHM).
//!
//! The negotiation/pod code follows wlx-capture's PipeWire backend (same
//! `pipewire` crate release), trimmed to what the viewer needs.
use std::os::fd::{FromRawFd, OwnedFd};
use std::sync::mpsc;
use std::sync::Once;
use std::thread::JoinHandle;

use drm_fourcc::DrmFourcc;
use pipewire as pw;
use pw::context::ContextRc;
use pw::main_loop::MainLoopRc;
use pw::properties::properties;
use pw::spa;
use pw::spa::buffer::meta::{MetaHeader, MetaHeaderFlags};
use pw::stream::{StreamFlags, StreamRc};
use spa::buffer::DataType;
use spa::param::video::{VideoFlags, VideoFormat, VideoInfoRaw};
use spa::param::ParamType;
use spa::pod::serialize::GenError;
use spa::pod::{ChoiceValue, Object, Pod, Property, PropertyFlags, Value};
use spa::utils::{Choice, ChoiceEnum, ChoiceFlags};

use super::dmabuf::DrmFormat;

#[derive(Clone, Copy, Debug)]
pub struct FrameFormat {
    pub width: u32,
    pub height: u32,
    pub fourcc: DrmFourcc,
    /// DRM format modifier; `DRM_FORMAT_MOD_INVALID` for SHM streams.
    pub modifier: u64,
}

pub struct Plane {
    pub fd: OwnedFd,
    pub offset: u32,
    pub stride: i32,
}

pub struct DmabufFrame {
    pub format: FrameFormat,
    pub planes: Vec<Plane>,
}

pub struct ShmFrame {
    pub format: FrameFormat,
    pub stride: i32,
    /// `stride * height` bytes, tightly owned (safe to keep past the callback).
    pub data: Vec<u8>,
}

pub enum Frame {
    Dmabuf(DmabufFrame),
    Shm(ShmFrame),
}

impl Frame {
    pub fn format(&self) -> FrameFormat {
        match self {
            Frame::Dmabuf(f) => f.format,
            Frame::Shm(f) => f.format,
        }
    }
}

enum Ctrl {
    Active(bool),
    Stop,
}

pub struct Capture {
    tx_ctrl: pw::channel::Sender<Ctrl>,
    rx_frame: mpsc::Receiver<Frame>,
    handle: Option<JoinHandle<()>>,
}

static PW_INIT: Once = Once::new();

impl Capture {
    /// Connect to `node_id` and start receiving. `formats` are the DMA-BUF
    /// (fourcc, modifier) pairs we can import; SHM is always offered too.
    pub fn start(name: String, node_id: u32, formats: Vec<DrmFormat>, max_fps: u32) -> Self {
        PW_INIT.call_once(pw::init);
        let (tx_frame, rx_frame) = mpsc::sync_channel(2);
        let (tx_ctrl, rx_ctrl) = pw::channel::channel();
        let handle = std::thread::Builder::new()
            .name(format!("pw-capture-{node_id}"))
            .spawn(move || {
                if let Err(e) = main_loop(name.clone(), node_id, formats, max_fps, tx_frame, rx_ctrl) {
                    log::error!("{name}: pipewire capture loop failed: {e}");
                }
            })
            .expect("spawn pipewire thread");
        Self { tx_ctrl, rx_frame, handle: Some(handle) }
    }

    /// Newest frame since the last call (older queued frames are dropped).
    pub fn latest(&self) -> Option<Frame> {
        self.rx_frame.try_iter().last()
    }

    pub fn set_active(&self, active: bool) {
        let _ = self.tx_ctrl.send(Ctrl::Active(active));
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.tx_ctrl.send(Ctrl::Stop);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn main_loop(
    name: String,
    node_id: u32,
    formats: Vec<DrmFormat>,
    max_fps: u32,
    sender: mpsc::SyncSender<Frame>,
    receiver: pw::channel::Receiver<Ctrl>,
) -> Result<(), pw::Error> {
    let main_loop = MainLoopRc::new(None)?;
    let context = ContextRc::new(&main_loop, None)?;
    let core = context.connect_rc(None)?;

    let stream = StreamRc::new(
        core,
        &name,
        properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )?;

    let _listener = stream
        .add_local_listener_with_user_data(FrameFormat {
            width: 0,
            height: 0,
            fourcc: DrmFourcc::Argb8888,
            modifier: MOD_INVALID,
        })
        .state_changed({
            let name = name.clone();
            move |_, _, old, new| log::info!("{name}: stream {old:?} -> {new:?}")
        })
        .param_changed({
            let name = name.clone();
            move |stream, format, id, param| {
                let Some(param) = param else { return };
                if id != ParamType::Format.as_raw() {
                    return;
                }
                let mut info = VideoInfoRaw::default();
                if let Err(e) = info.parse(param) {
                    log::warn!("{name}: bad video format param: {e:?}");
                    return;
                }
                format.width = info.size().width;
                format.height = info.size().height;
                format.fourcc = match spa_to_fourcc(info.format()) {
                    Some(f) => f,
                    None => {
                        log::warn!("{name}: unsupported video format {:?}", info.format());
                        return;
                    }
                };
                let dmabuf = info.flags().contains(VideoFlags::MODIFIER);
                format.modifier = if dmabuf { info.modifier() } else { MOD_INVALID };
                log::info!(
                    "{name}: format {}x{} {} modifier 0x{:016x} ({})",
                    format.width,
                    format.height,
                    format.fourcc,
                    format.modifier,
                    if dmabuf { "DMA-BUF" } else { "SHM" }
                );
                // Now that a format is fixed, say which buffer types + metas we take.
                let Ok(buf_bytes) = obj_to_bytes(buffer_params(dmabuf)) else { return };
                let Ok(hdr_bytes) = obj_to_bytes(meta_object(
                    spa::sys::SPA_META_Header,
                    std::mem::size_of::<spa::sys::spa_meta_header>(),
                )) else {
                    return;
                };
                let (Some(buf_pod), Some(hdr_pod)) = (Pod::from_bytes(&buf_bytes), Pod::from_bytes(&hdr_bytes)) else {
                    return;
                };
                let mut pods = [buf_pod, hdr_pod];
                if let Err(e) = stream.update_params(&mut pods) {
                    log::error!("{name}: update_params: {e}");
                }
            }
        })
        .process({
            let name = name.clone();
            move |stream, format| {
                // Keep only the newest queued buffer.
                let mut newest = None;
                while let Some(b) = stream.dequeue_buffer() {
                    newest = Some(b);
                }
                let Some(mut buffer) = newest else { return };
                if let Some(h) = buffer.find_meta::<MetaHeader>() {
                    if h.flags().contains(MetaHeaderFlags::CORRUPTED) {
                        return;
                    }
                }
                if format.width == 0 || format.height == 0 {
                    return;
                }
                let datas = buffer.datas_mut();
                if datas.is_empty() {
                    return;
                }
                let frame = match datas[0].type_() {
                    DataType::DmaBuf => {
                        let mut planes = Vec::with_capacity(datas.len());
                        for d in datas.iter() {
                            let raw = d.as_raw().fd as i32;
                            if raw < 0 {
                                return;
                            }
                            // Borrow → dup so the frame outlives the buffer requeue.
                            let borrowed = unsafe { OwnedFd::from_raw_fd(raw) };
                            let dup = borrowed.try_clone();
                            std::mem::forget(borrowed);
                            let Ok(fd) = dup else { return };
                            planes.push(Plane { fd, offset: d.chunk().offset(), stride: d.chunk().stride() });
                        }
                        Frame::Dmabuf(DmabufFrame { format: *format, planes })
                    }
                    DataType::MemFd | DataType::MemPtr => {
                        let d = &mut datas[0];
                        let stride = d.chunk().stride();
                        let offset = d.chunk().offset() as usize;
                        let len = stride.max(0) as usize * format.height as usize;
                        let data = match d.data() {
                            Some(mapped) => {
                                if mapped.len() < offset + len {
                                    return;
                                }
                                mapped[offset..offset + len].to_vec()
                            }
                            None => {
                                // Not mapped for us (shouldn't happen with MAP_BUFFERS) — mmap it.
                                let raw = d.as_raw();
                                let fd = raw.fd as i32;
                                if fd < 0 {
                                    return;
                                }
                                let map_len = raw.maxsize as usize + raw.mapoffset as usize;
                                let p = unsafe {
                                    libc::mmap(std::ptr::null_mut(), map_len, libc::PROT_READ, libc::MAP_SHARED, fd, 0)
                                };
                                if p == libc::MAP_FAILED {
                                    return;
                                }
                                let start = raw.mapoffset as usize + offset;
                                let v = unsafe { std::slice::from_raw_parts((p as *const u8).add(start), len) }.to_vec();
                                unsafe { libc::munmap(p, map_len) };
                                v
                            }
                        };
                        Frame::Shm(ShmFrame { format: *format, stride, data })
                    }
                    other => {
                        log::error!("{name}: unexpected buffer data type {other:?}");
                        return;
                    }
                };
                match sender.try_send(frame) {
                    Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
                    Err(mpsc::TrySendError::Disconnected(_)) => {
                        let _ = stream.disconnect();
                    }
                }
            }
        })
        .register()?;

    // Offer one EnumFormat per fourcc listing every importable modifier, then a
    // plain SHM alternative last (lowest priority).
    let mut by_fourcc: Vec<(DrmFourcc, Vec<u64>)> = Vec::new();
    for f in &formats {
        match by_fourcc.iter_mut().find(|(c, _)| *c == f.fourcc) {
            Some((_, mods)) => mods.push(f.modifier),
            None => by_fourcc.push((f.fourcc, vec![f.modifier])),
        }
    }
    let mut param_bytes: Vec<Vec<u8>> =
        by_fourcc.iter().filter_map(|(c, m)| obj_to_bytes(format_params(Some((*c, m)), max_fps)).ok()).collect();
    param_bytes.push(obj_to_bytes(format_params(None, max_fps)).expect("static SHM format params"));
    let mut params: Vec<&Pod> = param_bytes.iter().filter_map(|b| Pod::from_bytes(b)).collect();

    stream.connect(
        spa::utils::Direction::Input,
        Some(node_id),
        StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS,
        params.as_mut_slice(),
    )?;

    let _rx = receiver.attach(main_loop.loop_(), {
        let main_loop = main_loop.clone();
        let stream = stream.clone();
        move |req| match req {
            Ctrl::Active(a) => {
                if let Err(e) = stream.set_active(a) {
                    log::warn!("set_active({a}): {e}");
                }
            }
            Ctrl::Stop => main_loop.quit(),
        }
    });

    main_loop.run();
    log::info!("{name}: pipewire loop exited");
    Ok(())
}

pub const MOD_INVALID: u64 = 0x00ff_ffff_ffff_ffff;

fn obj_to_bytes(obj: Object) -> Result<Vec<u8>, GenError> {
    Ok(spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &Value::Object(obj))?
        .0
        .into_inner())
}

fn buffer_params(dmabuf: bool) -> Object {
    let data_types = if dmabuf {
        1 << DataType::DmaBuf.as_raw()
    } else {
        (1 << DataType::MemFd.as_raw()) | (1 << DataType::MemPtr.as_raw())
    };
    let property = Property {
        key: spa::sys::SPA_PARAM_BUFFERS_dataType,
        flags: PropertyFlags::empty(),
        value: Value::Int(data_types),
    };
    spa::pod::object!(spa::utils::SpaTypes::ObjectParamBuffers, ParamType::Buffers, property)
}

fn meta_object(key: u32, size: usize) -> Object {
    let ty = Property {
        key: spa::sys::SPA_PARAM_META_type,
        flags: PropertyFlags::empty(),
        value: Value::Id(spa::utils::Id(key)),
    };
    let sz = Property {
        key: spa::sys::SPA_PARAM_META_size,
        flags: PropertyFlags::empty(),
        value: Value::Int(size as i32),
    };
    spa::pod::object!(spa::utils::SpaTypes::ObjectParamMeta, ParamType::Meta, ty, sz)
}

fn format_params(fmt: Option<(DrmFourcc, &Vec<u64>)>, max_fps: u32) -> Object {
    let mut obj = spa::pod::object!(
        spa::utils::SpaTypes::ObjectParamFormat,
        ParamType::EnumFormat,
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaType,
            Id,
            spa::param::format::MediaType::Video
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaSubtype,
            Id,
            spa::param::format::MediaSubtype::Raw
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            spa::utils::Rectangle { width: 256, height: 256 },
            spa::utils::Rectangle { width: 1, height: 1 },
            spa::utils::Rectangle { width: 8192, height: 8192 }
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            spa::utils::Fraction { num: 0, denom: 1 },
            spa::utils::Fraction { num: 0, denom: 1 },
            spa::utils::Fraction { num: 1000, denom: 1 }
        ),
    );

    if max_fps > 0 {
        // Ask the compositor not to export more than this many frames per second
        // (KWin honours maxFramerate; others ignore it harmlessly).
        obj.properties.push(spa::pod::property!(
            spa::param::format::FormatProperties::VideoMaxFramerate,
            Choice,
            Range,
            Fraction,
            spa::utils::Fraction { num: max_fps, denom: 1 },
            spa::utils::Fraction { num: 1, denom: 1 },
            spa::utils::Fraction { num: max_fps, denom: 1 }
        ));
    }
    match fmt {
        Some((fourcc, mods)) => {
            let spa_fmt = fourcc_to_spa(fourcc);
            obj.properties.push(spa::pod::property!(
                spa::param::format::FormatProperties::VideoFormat,
                Choice,
                Enum,
                Id,
                spa_fmt,
                spa_fmt,
            ));
            obj.properties.push(Property {
                key: spa::param::format::FormatProperties::VideoModifier.as_raw(),
                flags: PropertyFlags::MANDATORY | PropertyFlags::DONT_FIXATE,
                value: Value::Choice(ChoiceValue::Long(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Enum {
                        default: mods[0] as i64,
                        alternatives: mods.iter().map(|m| *m as i64).collect(),
                    },
                ))),
            });
        }
        None => {
            obj.properties.push(spa::pod::property!(
                spa::param::format::FormatProperties::VideoFormat,
                Choice,
                Enum,
                Id,
                VideoFormat::BGRx,
                VideoFormat::BGRx,
                VideoFormat::BGRA,
                VideoFormat::RGBx,
                VideoFormat::RGBA,
            ));
        }
    }
    obj
}

fn fourcc_to_spa(f: DrmFourcc) -> VideoFormat {
    match f {
        DrmFourcc::Argb8888 => VideoFormat::BGRA,
        DrmFourcc::Xrgb8888 => VideoFormat::BGRx,
        DrmFourcc::Abgr8888 => VideoFormat::RGBA,
        DrmFourcc::Xbgr8888 => VideoFormat::RGBx,
        _ => VideoFormat::BGRx,
    }
}

#[allow(non_upper_case_globals)]
fn spa_to_fourcc(f: VideoFormat) -> Option<DrmFourcc> {
    Some(match f {
        VideoFormat::BGRA => DrmFourcc::Argb8888,
        VideoFormat::BGRx => DrmFourcc::Xrgb8888,
        VideoFormat::RGBA => DrmFourcc::Abgr8888,
        VideoFormat::RGBx => DrmFourcc::Xbgr8888,
        _ => return None,
    })
}
