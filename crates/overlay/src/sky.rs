//! 360° background: an equirectangular panorama shown as an
//! `XR_KHR_composition_layer_equirect2` layer behind everything while no game
//! is running, so an empty headset isn't a black void (SteamVR/WayVR do the
//! same). Built-in CC0 image, or any JPEG/PNG the user points the config at.
//!
//! The image is decoded off-thread, capped to `MAX_W`×`MAX_H`, and uploaded
//! once into a static-image swapchain.
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use ash::vk;
use ash::vk::Handle as _;
use openxr as xr;

static BUILTIN: &[u8] = include_bytes!("../assets/sky/table_mountain_2.jpg");

/// Upload cap: 4096×2048 RGBA8 = 32 MB of VRAM, plenty for a background.
const MAX_W: u32 = 4096;
const MAX_H: u32 = 2048;

pub struct Sky {
    rx: Option<mpsc::Receiver<Option<image::RgbaImage>>>,
    swapchain: Option<xr::Swapchain<xr::Vulkan>>,
    px: (u32, u32),
    /// Set once the swapchain holds the image (layer may be submitted).
    pub ready: bool,
    pub source: String,
}

/// Where a custom panorama is picked up without any config editing:
/// `~/.config/monadeck/skybox.{jpg,jpeg,png}`.
pub fn custom_path_hint() -> String {
    monadeck_core::paths::monadeck_config_dir().join("skybox.jpg").display().to_string()
}

/// The panorama to show: the configured `skybox_path` if set, else a file
/// dropped at the fixed spot, else the built-in image.
pub fn resolve_path(configured: Option<String>) -> Option<String> {
    if configured.is_some() {
        return configured;
    }
    let dir = monadeck_core::paths::monadeck_config_dir();
    ["skybox.jpg", "skybox.jpeg", "skybox.png"]
        .iter()
        .map(|n| dir.join(n))
        .find(|p| p.is_file())
        .map(|p| p.display().to_string())
}

impl Sky {
    /// Start decoding `path` (or the built-in panorama when None / unreadable).
    pub fn load(path: Option<String>) -> Self {
        let (tx, rx) = mpsc::channel();
        let source = path.clone().unwrap_or_else(|| "built-in (Table Mountain 2, CC0)".into());
        std::thread::Builder::new()
            .name("sky-decode".into())
            .spawn(move || {
                let bytes: std::borrow::Cow<[u8]> = match &path {
                    Some(p) => match std::fs::read(p) {
                        Ok(b) => b.into(),
                        Err(e) => {
                            log::warn!("sky: cannot read '{p}': {e}; using the built-in panorama");
                            BUILTIN.into()
                        }
                    },
                    None => BUILTIN.into(),
                };
                let img = match image::load_from_memory(&bytes) {
                    Ok(i) => i,
                    Err(e) => {
                        log::warn!("sky: cannot decode panorama: {e}");
                        let _ = tx.send(None);
                        return;
                    }
                };
                let (w, h) = (img.width(), img.height());
                let img = if w > MAX_W || h > MAX_H {
                    let s = (MAX_W as f32 / w as f32).min(MAX_H as f32 / h as f32);
                    img.resize_exact((w as f32 * s) as u32, (h as f32 * s) as u32, image::imageops::FilterType::Triangle)
                } else {
                    img
                };
                log::info!("sky: panorama {}x{} (source {w}x{h})", img.width(), img.height());
                let _ = tx.send(Some(img.into_rgba8()));
            })
            .expect("spawn sky decoder");
        Self { rx: Some(rx), swapchain: None, px: (0, 0), ready: false, source }
    }

    /// Poll the decoder; on arrival, create the swapchain and upload once.
    #[allow(clippy::too_many_arguments)]
    pub fn poll(
        &mut self,
        session: &xr::Session<xr::Vulkan>,
        device: &ash::Device,
        allocator: &Arc<Mutex<gpu_allocator::vulkan::Allocator>>,
        format: vk::Format,
        cmd: vk::CommandBuffer,
        queue: vk::Queue,
        fence: vk::Fence,
    ) -> Result<()> {
        let Some(rx) = &self.rx else { return Ok(()) };
        let img = match rx.try_recv() {
            Ok(Some(i)) => i,
            Ok(None) | Err(mpsc::TryRecvError::Disconnected) => {
                self.rx = None;
                return Ok(());
            }
            Err(mpsc::TryRecvError::Empty) => return Ok(()),
        };
        self.rx = None;
        let (w, h) = (img.width(), img.height());
        let mut swapchain = session.create_swapchain(&xr::SwapchainCreateInfo {
            create_flags: xr::SwapchainCreateFlags::STATIC_IMAGE,
            usage_flags: xr::SwapchainUsageFlags::COLOR_ATTACHMENT | xr::SwapchainUsageFlags::TRANSFER_DST,
            format: format.as_raw() as _,
            sample_count: 1,
            width: w,
            height: h,
            face_count: 1,
            array_size: 1,
            mip_count: 1,
        })?;
        let images: Vec<vk::Image> = swapchain.enumerate_images()?.into_iter().map(vk::Image::from_raw).collect();
        let index = swapchain.acquire_image()? as usize;
        swapchain.wait_image(xr::Duration::INFINITE)?;
        let dst = images[index];

        // Staging buffer (RGBA8, tightly packed) → image. Swizzle for BGRA targets.
        let bgra = matches!(format, vk::Format::B8G8R8A8_SRGB | vk::Format::B8G8R8A8_UNORM);
        let mut pixels = img.into_raw();
        if bgra {
            for p in pixels.chunks_exact_mut(4) {
                p.swap(0, 2);
            }
        }
        let size = pixels.len() as u64;
        let buffer = unsafe {
            device.create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(size)
                    .usage(vk::BufferUsageFlags::TRANSFER_SRC)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )?
        };
        let reqs = unsafe { device.get_buffer_memory_requirements(buffer) };
        let mut allocation = allocator
            .lock()
            .map_err(|_| anyhow!("allocator poisoned"))?
            .allocate(&gpu_allocator::vulkan::AllocationCreateDesc {
                name: "sky-staging",
                requirements: reqs,
                location: gpu_allocator::MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| anyhow!("sky staging alloc: {e}"))?;
        unsafe { device.bind_buffer_memory(buffer, allocation.memory(), allocation.offset())? };
        allocation.mapped_slice_mut().ok_or_else(|| anyhow!("sky staging not mapped"))?[..pixels.len()]
            .copy_from_slice(&pixels);

        let range = crate::desktop::dmabuf::color_range();
        unsafe {
            device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
            device.begin_command_buffer(
                cmd,
                &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
        }
        crate::desktop::dmabuf::cmd_transition(
            device,
            cmd,
            dst,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::AccessFlags::empty(),
            vk::AccessFlags::TRANSFER_WRITE,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::TRANSFER,
        );
        let region = vk::BufferImageCopy::default()
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: range.aspect_mask,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_extent(vk::Extent3D { width: w, height: h, depth: 1 });
        unsafe {
            device.cmd_copy_buffer_to_image(cmd, buffer, dst, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &[region]);
        }
        crate::desktop::dmabuf::cmd_transition(
            device,
            cmd,
            dst,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::AccessFlags::TRANSFER_WRITE,
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
        );
        unsafe {
            device.end_command_buffer(cmd)?;
            let cmds = [cmd];
            device.queue_submit(queue, &[vk::SubmitInfo::default().command_buffers(&cmds)], fence)?;
            device.wait_for_fences(&[fence], true, u64::MAX)?;
            device.reset_fences(&[fence])?;
        }
        if let Ok(mut al) = allocator.lock() {
            let _ = al.free(allocation);
        }
        unsafe { device.destroy_buffer(buffer, None) };
        swapchain.release_image()?;
        self.swapchain = Some(swapchain);
        self.px = (w, h);
        self.ready = true;
        log::info!("sky: background ready ({w}x{h})");
        Ok(())
    }

    /// Full-sphere equirect layer at infinity (radius 0).
    pub fn layer<'a>(&'a self, space: &'a xr::Space) -> Option<xr::CompositionLayerEquirect2KHR<'a, xr::Vulkan>> {
        let sc = self.swapchain.as_ref()?;
        if !self.ready {
            return None;
        }
        let sub = xr::SwapchainSubImage::new().swapchain(sc).image_array_index(0).image_rect(xr::Rect2Di {
            offset: xr::Offset2Di { x: 0, y: 0 },
            extent: xr::Extent2Di { width: self.px.0 as i32, height: self.px.1 as i32 },
        });
        Some(
            xr::CompositionLayerEquirect2KHR::new()
                .space(space)
                .eye_visibility(xr::EyeVisibility::BOTH)
                .sub_image(sub)
                .pose(xr::Posef::IDENTITY)
                .radius(0.0)
                .central_horizontal_angle(std::f32::consts::TAU)
                .upper_vertical_angle(std::f32::consts::FRAC_PI_2)
                .lower_vertical_angle(-std::f32::consts::FRAC_PI_2),
        )
    }
}
