//! `--scene-selftest [dir]`: the 3D layer drawn without a headset. A
//! headless Vulkan device renders a sample room (Index controllers mid-use, a
//! tracker, a glove hand, the floor grid and this machine's real base
//! stations) from a few viewpoints into PNGs, over a dark backdrop.
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use ash::vk;
use openxr as xr;

use super::{math, Buttons, Item, Scene, Slot, BASE_STATION_MODEL};
use crate::mathx::{normalize, quat_from_axes};

const W: u32 = 1280;
const H: u32 = 960;

pub fn run(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let entry = unsafe { ash::Entry::load() }.context("load vulkan")?;
    let app = vk::ApplicationInfo::default().api_version(vk::make_api_version(0, 1, 1, 0));
    let instance = unsafe { entry.create_instance(&vk::InstanceCreateInfo::default().application_info(&app), None) }.context("create instance")?;
    let phys = unsafe { instance.enumerate_physical_devices() }?
        .into_iter()
        .max_by_key(|&p| match unsafe { instance.get_physical_device_properties(p) }.device_type {
            vk::PhysicalDeviceType::DISCRETE_GPU => 3,
            vk::PhysicalDeviceType::INTEGRATED_GPU => 2,
            _ => 0,
        })
        .ok_or_else(|| anyhow!("no Vulkan device"))?;
    let qf = unsafe { instance.get_physical_device_queue_family_properties(phys) }
        .iter()
        .position(|q| q.queue_flags.contains(vk::QueueFlags::GRAPHICS))
        .ok_or_else(|| anyhow!("no graphics queue"))? as u32;
    let prio = [1.0f32];
    let qi = [vk::DeviceQueueCreateInfo::default().queue_family_index(qf).queue_priorities(&prio)];
    let device = unsafe { instance.create_device(phys, &vk::DeviceCreateInfo::default().queue_create_infos(&qi), None) }.context("create device")?;
    let queue = unsafe { device.get_device_queue(qf, 0) };
    let pool = unsafe {
        device.create_command_pool(&vk::CommandPoolCreateInfo::default().queue_family_index(qf).flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER), None)?
    };
    let cmd = unsafe { device.allocate_command_buffers(&vk::CommandBufferAllocateInfo::default().command_pool(pool).command_buffer_count(1))?[0] };
    let fence = unsafe { device.create_fence(&vk::FenceCreateInfo::default(), None)? };
    let allocator = Arc::new(Mutex::new(
        gpu_allocator::vulkan::Allocator::new(&gpu_allocator::vulkan::AllocatorCreateDesc {
            instance: instance.clone(),
            device: device.clone(),
            physical_device: phys,
            debug_settings: Default::default(),
            buffer_device_address: false,
            allocation_sizes: Default::default(),
        })
        .map_err(|e| anyhow!("gpu-allocator: {e}"))?,
    ));

    let format = vk::Format::R8G8B8A8_SRGB;
    let mut scene = Scene::new(&device, allocator.clone(), format, (W, H))?;

    // The offscreen colour image, and a buffer to read it back.
    let image = unsafe {
        device.create_image(
            &vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(format)
                .extent(vk::Extent3D { width: W, height: H, depth: 1 })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC)
                .initial_layout(vk::ImageLayout::UNDEFINED),
            None,
        )?
    };
    let reqs = unsafe { device.get_image_memory_requirements(image) };
    let ialloc = super::gpu::alloc_for_selftest(&allocator, reqs, false)?;
    unsafe { device.bind_image_memory(image, ialloc.memory(), ialloc.offset())? };
    let extent = vk::Extent2D { width: W, height: H };
    let target = scene.gpu.target(&[image], format, extent, 1)?;
    let readback = unsafe { device.create_buffer(&vk::BufferCreateInfo::default().size((W * H * 4) as u64).usage(vk::BufferUsageFlags::TRANSFER_DST), None)? };
    let breqs = unsafe { device.get_buffer_memory_requirements(readback) };
    let balloc = super::gpu::alloc_for_selftest(&allocator, breqs, true)?;
    unsafe { device.bind_buffer_memory(readback, balloc.memory(), balloc.offset())? };

    let items = sample();
    // Load every model first (the loader works off-thread).
    let refs: Vec<&Item> = items.iter().collect();
    let start = Instant::now();
    loop {
        scene.prepare(&refs, cmd, queue, fence)?;
        if !scene.models.values().any(|s| matches!(s, Slot::Loading)) {
            break;
        }
        if start.elapsed() > Duration::from_secs(30) {
            bail!("models still loading after 30 s");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    for name in &scene.missing {
        println!("missing model: {name}");
    }
    println!("models ready in {} ms", start.elapsed().as_millis());

    let fov = xr::Fovf { angle_left: -0.7, angle_right: 0.7, angle_up: 0.54, angle_down: -0.54 };
    let shots = [
        ("scene-hands", [0.0, 1.45, 0.15], [0.0, 1.05, -0.45]),
        ("scene-room", [0.4, 1.7, 3.2], [-0.6, 1.4, 0.0]),
        ("scene-top", [-0.7, 7.5, 0.0], [-0.7, 0.0, -0.001]),
    ];
    for (name, eye, at) in shots {
        let pose = look_at(eye, at);
        unsafe {
            device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
            device.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT))?;
        }
        let draws = scene.draws(&items);
        scene.gpu.record(cmd, target.framebuffers[0][0], extent, &math::view(&pose), &math::projection(&fov, 0.03, 100.0), &draws);
        crate::desktop::dmabuf::cmd_transition(
            &device,
            cmd,
            image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            vk::AccessFlags::TRANSFER_READ,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags::TRANSFER,
        );
        let region = vk::BufferImageCopy::default()
            .image_subresource(vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: 0, layer_count: 1 })
            .image_extent(vk::Extent3D { width: W, height: H, depth: 1 });
        unsafe {
            device.cmd_copy_image_to_buffer(cmd, image, vk::ImageLayout::TRANSFER_SRC_OPTIMAL, readback, &[region]);
            device.end_command_buffer(cmd)?;
            let cmds = [cmd];
            device.queue_submit(queue, &[vk::SubmitInfo::default().command_buffers(&cmds)], fence)?;
            device.wait_for_fences(&[fence], true, u64::MAX)?;
            device.reset_fences(&[fence])?;
        }
        let px = balloc.mapped_slice().ok_or_else(|| anyhow!("readback not mapped"))?;
        // Premultiplied over a dark backdrop, like a dimmed game behind.
        let mut out = image::RgbImage::new(W, H);
        for (i, p) in out.pixels_mut().enumerate() {
            let s = &px[i * 4..i * 4 + 4];
            let a = s[3] as f32 / 255.0;
            let bg = 28.0;
            *p = image::Rgb([0, 1, 2].map(|c| (s[c] as f32 + bg * (1.0 - a)).min(255.0) as u8));
        }
        let path = dir.join(format!("{name}.png"));
        out.save(&path)?;
        println!("{}", path.display());
    }
    Ok(())
}

/// An eye at `eye` looking at `at` (+Y up, or -Z when looking straight down).
fn look_at(eye: [f32; 3], at: [f32; 3]) -> xr::Posef {
    let f = normalize([at[0] - eye[0], at[1] - eye[1], at[2] - eye[2]]);
    let z = [-f[0], -f[1], -f[2]];
    let up = if f[1].abs() > 0.99 { [0.0, 0.0, -1.0] } else { [0.0, 1.0, 0.0] };
    let x = normalize(crate::mathx::cross(up, z));
    let y = crate::mathx::cross(z, x);
    math::pose(eye, quat_from_axes(x, y, z))
}

/// The sample room: what the overlay would show with the dashboard open.
fn sample() -> Vec<Item> {
    let tilt = |deg: f32| crate::mathx::quat_from_axis_angle([1.0, 0.0, 0.0], deg.to_radians());
    let mut items = vec![Item::Grid { stage: xr::Posef::IDENTITY, at: [0.0, -0.3] }];
    for s in monadeck_core::room_setup::base_stations() {
        let p = s.position.map(|v| v as f32);
        let q = s.orientation.map(|v| v as f32);
        println!("base station {} at {:.2?}", s.serial, p);
        items.push(Item::Model { name: BASE_STATION_MODEL.into(), pose: math::pose(p, q), buttons: None, alpha: 0.85 });
    }
    items.push(Item::Model {
        name: "{tundra_labs}tundra_tracker".into(),
        pose: math::pose([0.12, 0.95, -0.55], tilt(-80.0)),
        buttons: None,
        alpha: 0.85,
    });
    // Controllers held at rest, grips pointing slightly forward.
    let left = Buttons { stick: (0.6, 0.8), ..Default::default() };
    let right = Buttons { trigger: 1.0, a: true, ..Default::default() };
    items.push(Item::Controller {
        name: super::controller_model("knuckles", false, "").unwrap(),
        grip: math::pose([-0.17, 1.02, -0.38], tilt(-20.0)),
        buttons: left,
    });
    items.push(Item::Controller {
        name: super::controller_model("knuckles", true, "").unwrap(),
        grip: math::pose([0.17, 1.02, -0.38], tilt(-20.0)),
        buttons: right,
    });
    items.push(Item::Hand(Box::new(sample_hand([0.36, 1.12, -0.55]))));
    items
}

/// A flat right hand reaching forward, palm down (stand-in for a glove).
fn sample_hand(wrist: [f32; 3]) -> xr::HandJointLocations {
    let mut joints = [xr::HandJointLocation {
        location_flags: xr::SpaceLocationFlags::POSITION_VALID | xr::SpaceLocationFlags::ORIENTATION_VALID,
        pose: xr::Posef::IDENTITY,
        radius: 0.008,
    }; 26];
    let at = |x: f32, z: f32| math::pose([wrist[0] + x, wrist[1], wrist[2] - z], [0.0, 0.0, 0.0, 1.0]);
    joints[0].pose = at(0.0, 0.05);
    joints[1].pose = at(0.0, 0.0);
    // Thumb, then index..little: metacarpal, proximal, intermediate, distal, tip.
    let fingers = [(-0.035, 4), (-0.02, 5), (0.0, 5), (0.02, 5), (0.038, 5)];
    let mut j = 2;
    for (fi, (x, n)) in fingers.into_iter().enumerate() {
        for k in 0..n {
            let reach = if fi == 0 { 0.02 + 0.022 * k as f32 } else { 0.03 + 0.028 * k as f32 };
            let spread = if fi == 0 { x - 0.01 * k as f32 } else { x * (1.0 + 0.15 * k as f32) };
            joints[j].pose = at(spread, reach);
            joints[j].radius = 0.009 - 0.0008 * k as f32;
            j += 1;
        }
    }
    joints
}
