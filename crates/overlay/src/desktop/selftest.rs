//! Headless check of the desktop-viewer pipeline: monitor layout, uinput,
//! portal approval, PipeWire frames, DMA-BUF import + blit on a plain Vulkan
//! device, and a PNG dump of the first frame. `monadeck-overlay --desktop-selftest`.
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use ash::vk;

use super::dmabuf::{self, Caps, Importer};
use super::pw::{Capture, Frame};
use super::{hid, outputs, portal};

pub fn run() -> Result<()> {
    println!("== outputs (wayland) ==");
    let outs = outputs::list();
    for o in &outs {
        println!("  {} {:?} at {:?} logical {:?} px {:?}", o.name, o.description, o.logical_pos, o.logical_size, o.pixel_size);
    }
    if outs.is_empty() {
        println!("  (none — not a Wayland session, or xdg_output missing)");
    }

    println!("== uinput ==");
    match hid::UInput::open() {
        Ok(_) => println!("  ok: virtual mouse + keyboard created"),
        Err(e) => println!("  unavailable: {e}"),
    }

    println!("== vulkan ==");
    let entry = unsafe { ash::Entry::load() }.context("load vulkan")?;
    let app = vk::ApplicationInfo::default().api_version(vk::make_api_version(0, 1, 1, 0));
    let instance = unsafe { entry.create_instance(&vk::InstanceCreateInfo::default().application_info(&app), None) }
        .context("create instance")?;
    let phys_devs = unsafe { instance.enumerate_physical_devices() }?;
    let phys = phys_devs
        .iter()
        .copied()
        .max_by_key(|&p| {
            let props = unsafe { instance.get_physical_device_properties(p) };
            match props.device_type {
                vk::PhysicalDeviceType::DISCRETE_GPU => 3,
                vk::PhysicalDeviceType::INTEGRATED_GPU => 2,
                vk::PhysicalDeviceType::VIRTUAL_GPU => 1,
                _ => 0,
            }
        })
        .ok_or_else(|| anyhow!("no Vulkan device"))?;
    let props = unsafe { instance.get_physical_device_properties(phys) };
    println!("  device: {}", props.device_name_as_c_str().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
    let qf = unsafe { instance.get_physical_device_queue_family_properties(phys) }
        .iter()
        .position(|q| q.queue_flags.contains(vk::QueueFlags::GRAPHICS))
        .ok_or_else(|| anyhow!("no graphics queue"))? as u32;
    let (exts, dmabuf_ok) = dmabuf::available_extensions(&instance, phys);
    println!("  dma-buf extensions: {}", if dmabuf_ok { "all present" } else { "MISSING (SHM only)" });
    let prio = [1.0f32];
    let qi = [vk::DeviceQueueCreateInfo::default().queue_family_index(qf).queue_priorities(&prio)];
    let device = unsafe {
        instance.create_device(
            phys,
            &vk::DeviceCreateInfo::default().queue_create_infos(&qi).enabled_extension_names(&exts),
            None,
        )
    }
    .context("create device")?;
    let queue = unsafe { device.get_device_queue(qf, 0) };
    // UNORM end-to-end here so the blit is a byte copy and the PNG is faithful.
    let fmt = vk::Format::B8G8R8A8_UNORM;
    let caps = Caps::query(&instance, phys, dmabuf_ok, fmt);
    println!("  importable format/modifier pairs: {}", caps.formats.len());
    for f in caps.formats.iter().take(12) {
        println!("    {} 0x{:016x}", f.fourcc, f.modifier);
    }
    let importer = caps.dmabuf.then(|| Importer::new(&instance, &device, qf, fmt));

    println!("== portal (approve the screen-share dialog on your desktop) ==");
    let rx = portal::start(None);
    let cast = match rx.recv_timeout(Duration::from_secs(90)) {
        Ok(Ok(c)) => c,
        Ok(Err(e)) => bail!("portal: {e}"),
        Err(_) => bail!("portal: no answer within 90 s (dialog not approved?)"),
    };
    println!("  restore token: {}", cast.restore_token.as_deref().map(|t| format!("{}…", &t[..t.len().min(8)])).unwrap_or("none".into()));
    for s in &cast.streams {
        println!("  stream node {} pos {:?} size {:?} mapping {:?}", s.node_id, s.position, s.size, s.mapping_id);
    }
    let Some(first) = cast.streams.first() else { bail!("no streams") };

    println!("== pipewire ==");
    let capture = Capture::start("selftest".into(), first.node_id, caps.formats.clone());
    let start = Instant::now();
    let mut got: Option<Frame> = None;
    let mut count = 0u32;
    while start.elapsed() < Duration::from_secs(8) {
        if let Some(f) = capture.latest() {
            count += 1;
            let ff = f.format();
            if count <= 3 {
                let kind = match &f {
                    Frame::Dmabuf(d) => format!("DMA-BUF {} plane(s), stride {}", d.planes.len(), d.planes[0].stride),
                    Frame::Shm(s) => format!("SHM stride {} ({} bytes)", s.stride, s.data.len()),
                };
                println!("  frame {}x{} {} mod 0x{:016x}: {kind}", ff.width, ff.height, ff.fourcc, ff.modifier);
            }
            got = Some(f);
            if count >= 30 {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let Some(frame) = got else { bail!("no frames received in 8 s") };
    println!("  {count} frame(s) in {:.1} s", start.elapsed().as_secs_f32());

    println!("== gpu import + readback ==");
    let ff = frame.format();
    let (w, h) = (ff.width, ff.height);
    let pool = unsafe {
        device.create_command_pool(
            &vk::CommandPoolCreateInfo::default().queue_family_index(qf).flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
            None,
        )?
    };
    let cmd = unsafe {
        device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default().command_pool(pool).level(vk::CommandBufferLevel::PRIMARY).command_buffer_count(1),
        )?[0]
    };
    let fence = unsafe { device.create_fence(&vk::FenceCreateInfo::default(), None)? };

    let rgba: Vec<u8> = match &frame {
        Frame::Dmabuf(d) => {
            let imp = importer.as_ref().ok_or_else(|| anyhow!("DMA-BUF frame without import support"))?;
            let img = imp.import(d).context("import")?;
            println!("  imported {}x{} as VkImage", img.extent.width, img.extent.height);
            // Host-readable linear destination.
            let dst = unsafe {
                device.create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .format(fmt)
                        .extent(vk::Extent3D { width: w, height: h, depth: 1 })
                        .mip_levels(1)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::LINEAR)
                        .usage(vk::ImageUsageFlags::TRANSFER_DST)
                        .initial_layout(vk::ImageLayout::UNDEFINED),
                    None,
                )?
            };
            let reqs = unsafe { device.get_image_memory_requirements(dst) };
            let mem_props = unsafe { instance.get_physical_device_memory_properties(phys) };
            let want = vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;
            let ty = (0..mem_props.memory_type_count)
                .find(|&i| reqs.memory_type_bits & (1 << i) != 0 && mem_props.memory_types[i as usize].property_flags.contains(want))
                .ok_or_else(|| anyhow!("no host-visible memory for readback"))?;
            let mem = unsafe {
                device.allocate_memory(&vk::MemoryAllocateInfo::default().allocation_size(reqs.size).memory_type_index(ty), None)?
            };
            unsafe { device.bind_image_memory(dst, mem, 0)? };
            unsafe {
                device.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT))?;
            }
            dmabuf::cmd_transition(
                &device, cmd, dst,
                vk::ImageLayout::UNDEFINED, vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::AccessFlags::empty(), vk::AccessFlags::TRANSFER_WRITE,
                vk::PipelineStageFlags::TOP_OF_PIPE, vk::PipelineStageFlags::TRANSFER,
            );
            imp.cmd_blit(cmd, &img, dst, vk::Extent2D { width: w, height: h });
            dmabuf::cmd_transition(
                &device, cmd, dst,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::ImageLayout::GENERAL,
                vk::AccessFlags::TRANSFER_WRITE, vk::AccessFlags::HOST_READ,
                vk::PipelineStageFlags::TRANSFER, vk::PipelineStageFlags::HOST,
            );
            unsafe {
                device.end_command_buffer(cmd)?;
                let cmds = [cmd];
                device.queue_submit(queue, &[vk::SubmitInfo::default().command_buffers(&cmds)], fence)?;
                device.wait_for_fences(&[fence], true, u64::MAX)?;
            }
            let layout = unsafe {
                device.get_image_subresource_layout(
                    dst,
                    vk::ImageSubresource { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, array_layer: 0 },
                )
            };
            let ptr = unsafe { device.map_memory(mem, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())? } as *const u8;
            let mut out = Vec::with_capacity((w * h * 4) as usize);
            for y in 0..h as usize {
                let row = unsafe {
                    std::slice::from_raw_parts(ptr.add(layout.offset as usize + y * layout.row_pitch as usize), (w * 4) as usize)
                };
                out.extend_from_slice(row);
            }
            unsafe {
                device.unmap_memory(mem);
                device.destroy_image(dst, None);
                device.free_memory(mem, None);
            }
            imp.destroy(img);
            bgra_to_rgba(out)
        }
        Frame::Shm(s) => {
            println!("  SHM frame (no GPU import needed)");
            let mut out = Vec::with_capacity((w * h * 4) as usize);
            for y in 0..h as usize {
                out.extend_from_slice(&s.data[y * s.stride as usize..y * s.stride as usize + (w * 4) as usize]);
            }
            if dmabuf::fourcc_is_rgba_order(ff.fourcc) {
                out
            } else {
                bgra_to_rgba(out)
            }
        }
    };
    let path = std::env::var("MONADECK_SELFTEST_PNG").unwrap_or_else(|_| "/tmp/monadeck-desktop-selftest.png".into());
    image::RgbaImage::from_raw(w, h, rgba)
        .ok_or_else(|| anyhow!("bad image size"))?
        .save(&path)
        .with_context(|| format!("write {path}"))?;
    println!("  wrote {path}");
    unsafe {
        device.destroy_fence(fence, None);
        device.destroy_command_pool(pool, None);
    }
    drop(capture);
    drop(cast);
    unsafe {
        device.destroy_device(None);
        instance.destroy_instance(None);
    }
    println!("== OK ==");
    Ok(())
}

fn bgra_to_rgba(mut v: Vec<u8>) -> Vec<u8> {
    for px in v.chunks_exact_mut(4) {
        px.swap(0, 2);
        px[3] = 255;
    }
    v
}
