// monadeck-overlay — in-headset VR game-library launcher for Monado.
//
// An OpenXR overlay session (XR_EXTX_overlay) that renders one large
// SteamVR-dashboard-style egui panel as a composition-layer quad, with a
// controller laser pointer for selection and grip-to-move. Game discovery and
// cover art come from monadeck-core (shared with the desktop launcher).
//
// The OpenXR/Vulkan/egui/laser plumbing is adapted from monado-frame.

mod games;
mod gfx;
mod mathx;
mod ui;

use std::os::raw::c_char;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{bail, Result};
use ash::vk;
use ash::vk::Handle as _;
use openxr as xr;

use gfx::{
    cylinder_layer, cylinder_params, fill_laser, laser_quad, make_laser, make_panel, quad_layer,
    render_panel,
};
use mathx::{front_pose, locate_pose, pose_compose, pose_invert, posef, raycast, raycast_cylinder};

static VK_ENTRY: OnceLock<ash::Entry> = OnceLock::new();

const GRAB_START: f32 = 0.40; // grip FORCE to start moving the panel
const GRAB_RELEASE: f32 = 0.15;
/// Curve radius for the cylinder panel (m). Equals the panel's anchor distance,
/// so the cylinder axis lands at the viewer — a SteamVR-style wrap.
const CURVE_RADIUS: f32 = 1.5;

unsafe extern "system" fn get_instance_proc_addr(
    instance: xr::sys::platform::VkInstance,
    name: *const c_char,
) -> Option<unsafe extern "system" fn()> {
    let entry = VK_ENTRY.get().expect("vk entry not initialised");
    let vk_instance = vk::Instance::from_raw(instance as _);
    (entry.static_fn().get_instance_proc_addr)(vk_instance, name)
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    if let Err(e) = run() {
        log::error!("overlay exited with error: {e:?}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // --- OpenXR instance (overlay) ------------------------------------------
    let entry = xr::Entry::linked();
    let available = entry.enumerate_extensions()?;
    if !available.khr_vulkan_enable2 {
        bail!("runtime is missing XR_KHR_vulkan_enable2");
    }
    if !available.extx_overlay {
        bail!("runtime is missing XR_EXTX_overlay");
    }
    // Curved (cylinder) panel when the runtime supports it — the SteamVR look.
    // MONADECK_OVERLAY_FLAT forces the flat quad (debugging / comparison).
    let curved = available.khr_composition_layer_cylinder && std::env::var("MONADECK_OVERLAY_FLAT").is_err();
    let mut exts = xr::ExtensionSet::default();
    exts.khr_vulkan_enable2 = true;
    exts.extx_overlay = true;
    exts.khr_composition_layer_cylinder = curved;
    let xr_instance = entry.create_instance(
        &xr::ApplicationInfo {
            api_version: xr::Version::new(1, 0, 32),
            application_name: "monadeck-overlay",
            application_version: 0,
            engine_name: "monadeck-overlay",
            engine_version: 0,
        },
        &exts,
        &[],
    )?;
    let props = xr_instance.properties()?;
    log::info!("OpenXR runtime: {} {}", props.runtime_name, props.runtime_version);
    let system = xr_instance.system(xr::FormFactor::HEAD_MOUNTED_DISPLAY)?;
    let _reqs = xr_instance.graphics_requirements::<xr::Vulkan>(system)?;
    let blend_mode = xr_instance
        .enumerate_environment_blend_modes(system, xr::ViewConfigurationType::PRIMARY_STEREO)?
        .first()
        .copied()
        .unwrap_or(xr::EnvironmentBlendMode::OPAQUE);

    // --- Vulkan via XR ------------------------------------------------------
    let vk_entry = unsafe { ash::Entry::load() }?;
    VK_ENTRY.set(vk_entry).ok();
    let app_info = vk::ApplicationInfo::default().api_version(vk::make_api_version(0, 1, 1, 0));
    let vk_instance_raw = unsafe {
        xr_instance
            .create_vulkan_instance(
                system,
                get_instance_proc_addr,
                std::ptr::from_ref(&vk::InstanceCreateInfo::default().application_info(&app_info)).cast(),
            )?
            .map_err(vk::Result::from_raw)?
    };
    let vk_instance = unsafe {
        ash::Instance::load(
            VK_ENTRY.get().unwrap().static_fn(),
            vk::Instance::from_raw(vk_instance_raw as _),
        )
    };
    let phys_raw = unsafe { xr_instance.vulkan_graphics_device(system, vk_instance_raw as _)? };
    let physical_device = vk::PhysicalDevice::from_raw(phys_raw as _);
    let queue_family_index = unsafe {
        vk_instance
            .get_physical_device_queue_family_properties(physical_device)
            .iter()
            .enumerate()
            .find(|(_, q)| q.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .map(|(i, _)| i as u32)
            .ok_or_else(|| anyhow::anyhow!("no graphics queue family"))?
    };
    let priorities = [1.0f32];
    let queue_infos =
        [vk::DeviceQueueCreateInfo::default().queue_family_index(queue_family_index).queue_priorities(&priorities)];
    let device_create_info = vk::DeviceCreateInfo::default().queue_create_infos(&queue_infos);
    let vk_device_raw = unsafe {
        xr_instance
            .create_vulkan_device(
                system,
                get_instance_proc_addr,
                phys_raw as _,
                std::ptr::from_ref(&device_create_info).cast(),
            )?
            .map_err(vk::Result::from_raw)?
    };
    let device = unsafe { ash::Device::load(vk_instance.fp_v1_0(), vk::Device::from_raw(vk_device_raw as _)) };
    let queue = unsafe { device.get_device_queue(queue_family_index, 0) };

    let cmd_pool = unsafe {
        device.create_command_pool(
            &vk::CommandPoolCreateInfo::default()
                .queue_family_index(queue_family_index)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
            None,
        )?
    };
    let cmd = unsafe {
        device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(cmd_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1),
        )?[0]
    };
    let fence = unsafe { device.create_fence(&vk::FenceCreateInfo::default(), None)? };

    // --- Overlay session ----------------------------------------------------
    let (session, mut frame_waiter, mut frame_stream) = unsafe {
        let raw = create_overlay_session(
            &xr_instance,
            system,
            &xr::vulkan::SessionCreateInfo {
                instance: vk_instance_raw as _,
                physical_device: phys_raw as _,
                device: vk_device_raw as _,
                queue_family_index,
                queue_index: 0,
            },
        )
        .map_err(|e| anyhow::anyhow!("xrCreateSession (overlay) failed: {:?}", e))?;
        xr::Session::<xr::Vulkan>::from_raw(xr_instance.clone(), raw, Box::new(()))
    };
    let space = session.create_reference_space(xr::ReferenceSpaceType::LOCAL, xr::Posef::IDENTITY)?;
    let view_space = session.create_reference_space(xr::ReferenceSpaceType::VIEW, xr::Posef::IDENTITY)?;

    // --- Format + render pass + allocator -----------------------------------
    let formats = session.enumerate_swapchain_formats()?;
    let preferred = [
        vk::Format::B8G8R8A8_SRGB,
        vk::Format::R8G8B8A8_SRGB,
        vk::Format::B8G8R8A8_UNORM,
        vk::Format::R8G8B8A8_UNORM,
    ];
    let format = preferred
        .into_iter()
        .find(|w| formats.iter().any(|f| (*f as i64) == (w.as_raw() as i64)))
        .unwrap_or(vk::Format::B8G8R8A8_SRGB);
    let srgb = matches!(format, vk::Format::B8G8R8A8_SRGB | vk::Format::R8G8B8A8_SRGB);
    let alpha_mode = false; // solid SteamVR-style panel
    log::info!("swapchain format {:?} srgb={} curved={}", format, srgb, curved);

    let color_attachment = vk::AttachmentDescription::default()
        .format(format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
    let color_ref = [vk::AttachmentReference::default().attachment(0).layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
    let subpass = [vk::SubpassDescription::default().pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS).color_attachments(&color_ref)];
    let attachments = [color_attachment];
    let render_pass = unsafe {
        device.create_render_pass(
            &vk::RenderPassCreateInfo::default().attachments(&attachments).subpasses(&subpass),
            None,
        )?
    };

    let allocator = Arc::new(Mutex::new(
        gpu_allocator::vulkan::Allocator::new(&gpu_allocator::vulkan::AllocatorCreateDesc {
            instance: vk_instance.clone(),
            device: device.clone(),
            physical_device,
            debug_settings: Default::default(),
            buffer_device_address: false,
            allocation_sizes: Default::default(),
        })
        .map_err(|e| anyhow::anyhow!("gpu-allocator init: {e}"))?,
    ));

    // One wide library panel ~1.3 m across, anchored ahead until grabbed.
    let mut panel = make_panel(
        &session,
        &device,
        allocator.clone(),
        render_pass,
        format,
        srgb,
        (2000, 1250),
        (1.3, 1.3 * 1250.0 / 2000.0),
        posef([0.0, 0.0, -1.5]),
    )?;
    let mut laser = make_laser(&session, format)?;

    // --- Actions ------------------------------------------------------------
    let action_set = xr_instance.create_action_set("monadeck", "monadeck overlay controls", 0)?;
    let left_path = xr_instance.string_to_path("/user/hand/left")?;
    let right_path = xr_instance.string_to_path("/user/hand/right")?;
    let aim_action = action_set.create_action::<xr::Posef>("aim", "Aim pose", &[left_path, right_path])?;
    let select_action = action_set.create_action::<f32>("select", "Select", &[left_path, right_path])?;
    let grab_action = action_set.create_action::<f32>("grab", "Grab", &[left_path, right_path])?;
    let scroll_action = action_set.create_action::<xr::Vector2f>("scroll", "Scroll", &[left_path, right_path])?;
    let system_action = action_set.create_action::<bool>("recenter", "Recenter panel", &[left_path, right_path])?;
    let index_profile = xr_instance.string_to_path("/interaction_profiles/valve/index_controller")?;
    xr_instance.suggest_interaction_profile_bindings(
        index_profile,
        &[
            xr::Binding::new(&aim_action, xr_instance.string_to_path("/user/hand/left/input/aim/pose")?),
            xr::Binding::new(&aim_action, xr_instance.string_to_path("/user/hand/right/input/aim/pose")?),
            xr::Binding::new(&select_action, xr_instance.string_to_path("/user/hand/left/input/trigger/value")?),
            xr::Binding::new(&select_action, xr_instance.string_to_path("/user/hand/right/input/trigger/value")?),
            xr::Binding::new(&grab_action, xr_instance.string_to_path("/user/hand/left/input/squeeze/force")?),
            xr::Binding::new(&grab_action, xr_instance.string_to_path("/user/hand/right/input/squeeze/force")?),
            xr::Binding::new(&scroll_action, xr_instance.string_to_path("/user/hand/left/input/thumbstick")?),
            xr::Binding::new(&scroll_action, xr_instance.string_to_path("/user/hand/right/input/thumbstick")?),
            xr::Binding::new(&system_action, xr_instance.string_to_path("/user/hand/left/input/system/click")?),
            xr::Binding::new(&system_action, xr_instance.string_to_path("/user/hand/right/input/system/click")?),
        ],
    )?;
    session.attach_action_sets(&[&action_set])?;
    let aim_left = aim_action.create_space(&session, left_path, xr::Posef::IDENTITY)?;
    let aim_right = aim_action.create_space(&session, right_path, xr::Posef::IDENTITY)?;

    // --- Game scan (background) ---------------------------------------------
    let (scan_tx, scan_rx) = std::sync::mpsc::channel::<Vec<games::GameEntry>>();
    std::thread::spawn(move || {
        let entries = games::scan();
        log::info!("scan found {} games", entries.len());
        let _ = scan_tx.send(entries);
    });

    let mut st = ui::LibState::new();

    // --- Loop state ---------------------------------------------------------
    let mut events = xr::EventDataBuffer::new();
    let mut running = false;
    let mut focused = false;
    let mut recenter = true;
    let mut sys_prev = false;
    // (hand index, controller->panel offset) while grabbing.
    let mut grab: Option<(usize, xr::Posef)> = None;

    log::info!("monadeck-overlay ready. Point to interact, grip to move, trigger to select.");

    loop {
        while let Some(event) = xr_instance.poll_event(&mut events)? {
            use xr::Event::*;
            match event {
                SessionStateChanged(e) => {
                    log::info!("session state -> {:?}", e.state());
                    focused = e.state() == xr::SessionState::FOCUSED;
                    match e.state() {
                        xr::SessionState::READY => {
                            session.begin(xr::ViewConfigurationType::PRIMARY_STEREO)?;
                            running = true;
                        }
                        xr::SessionState::STOPPING => {
                            session.end()?;
                            running = false;
                        }
                        xr::SessionState::EXITING | xr::SessionState::LOSS_PENDING => return Ok(()),
                        _ => {}
                    }
                }
                InstanceLossPending(_) => return Ok(()),
                _ => {}
            }
        }
        if !running {
            std::thread::sleep(std::time::Duration::from_millis(100));
            continue;
        }

        let frame_state = frame_waiter.wait()?;
        frame_stream.begin()?;
        if !frame_state.should_render {
            frame_stream.end(frame_state.predicted_display_time, blend_mode, &[])?;
            continue;
        }
        let time = frame_state.predicted_display_time;
        let hmd = locate_pose(&view_space, &space, time);

        // Drain finished scan -> upload cover textures into the panel context.
        if let Ok(entries) = scan_rx.try_recv() {
            st.games = games::load_into(&panel.ctx, entries);
            st.scanning = false;
            if st.selected.is_none() && !st.games.is_empty() {
                st.selected = Some(0);
            }
        }

        // Place the panel in front of the head on first frame / on recenter.
        if recenter {
            if let Some(h) = hmd {
                panel.pose = front_pose(&h, CURVE_RADIUS, 0.0, 0.0);
                recenter = false;
            }
        }

        // --- Input: laser hit-test + grip-to-move ---------------------------
        let mut pointer: Option<(f32, f32, bool)> = None;
        let mut laser_ray: Option<(xr::Posef, f32)> = None;
        let mut scroll = (0.0f32, 0.0f32);
        if focused {
            session.sync_actions(&[(&action_set).into()])?;

            // System click (either hand, rising edge) recenters the panel.
            let sys = system_action.state(&session, left_path)?.current_state
                || system_action.state(&session, right_path)?.current_state;
            if sys && !sys_prev {
                recenter = true;
            }
            sys_prev = sys;

            // Continue an in-progress grab.
            if let Some((hand_i, offset)) = grab {
                let (path, aim) = if hand_i == 0 { (left_path, &aim_left) } else { (right_path, &aim_right) };
                let grip = grab_action.state(&session, path)?.current_state;
                if grip < GRAB_RELEASE {
                    grab = None;
                } else if let Some(p) = locate_pose(aim, &space, time) {
                    panel.pose = pose_compose(&p, &offset);
                }
            }

            if grab.is_none() {
                let (cyl_pose, cyl_angle, cyl_height) = cylinder_params(&panel, CURVE_RADIUS);
                let mut best_t = f32::MAX;
                for (idx, (aim, path)) in [(&aim_left, left_path), (&aim_right, right_path)].into_iter().enumerate() {
                    let Some(p) = locate_pose(aim, &space, time) else { continue };
                    let hit = if curved {
                        raycast_cylinder(&p, &cyl_pose, CURVE_RADIUS, cyl_angle, cyl_height)
                    } else {
                        raycast(&p, &panel.pose, panel.size_m)
                    };
                    let Some((u, v, t)) = hit else { continue };
                    // Start grabbing if squeezing while pointed at the panel.
                    let grip = grab_action.state(&session, path)?.current_state;
                    if grip > GRAB_START {
                        grab = Some((idx, pose_compose(&pose_invert(&p), &panel.pose)));
                        pointer = None;
                        laser_ray = None;
                        break;
                    }
                    if t < best_t {
                        best_t = t;
                        let down = select_action.state(&session, path)?.current_state > 0.5;
                        pointer = Some((u, v, down));
                        laser_ray = Some((p, t));
                        // Thumbstick on the pointing hand scrolls the list.
                        let s = scroll_action.state(&session, path)?.current_state;
                        scroll = deadzone(s.x, s.y);
                    }
                }
            }
        }

        // --- Render + submit ------------------------------------------------
        render_panel(
            &mut panel,
            &device,
            render_pass,
            cmd,
            cmd_pool,
            queue,
            fence,
            alpha_mode,
            pointer,
            scroll,
            |ctx| ui::build(ctx, &mut st),
        )?;

        if laser_ray.is_some() {
            fill_laser(&mut laser, &device, cmd, queue, fence)?;
        }

        // The panel as a cylinder (curved) or quad (flat) layer — declared out
        // here so the chosen one outlives the layer-pointer vec.
        let panel_quad;
        let panel_cyl;
        let laser_q = match (laser_ray, hmd) {
            (Some((aim, t)), Some(h)) => Some(laser_quad(&laser, &space, &aim, t, &h)),
            _ => None,
        };
        let mut layers: Vec<&xr::CompositionLayerBase<xr::Vulkan>> = Vec::new();
        if curved {
            panel_cyl = cylinder_layer(&panel, &space, CURVE_RADIUS);
            layers.push(&panel_cyl);
        } else {
            panel_quad = quad_layer(&panel, &space, alpha_mode);
            layers.push(&panel_quad);
        }
        if let Some(q) = &laser_q {
            layers.push(q);
        }
        frame_stream.end(time, blend_mode, &layers)?;

        // --- Drain UI actions -----------------------------------------------
        if let Some(i) = st.launch_request.take() {
            if let Some(g) = st.games.get(i) {
                launch_game(g);
            }
        }
        if st.recenter_request {
            st.recenter_request = false;
            recenter = true;
        }
    }
}

/// Apply a radial deadzone + rescale to a thumbstick reading, so a resting stick
/// reads zero and the live range stays full-throw.
fn deadzone(x: f32, y: f32) -> (f32, f32) {
    const DZ: f32 = 0.2;
    let mag = (x * x + y * y).sqrt();
    if mag < DZ {
        return (0.0, 0.0);
    }
    let scale = ((mag - DZ) / (1.0 - DZ)) / mag;
    (x * scale, y * scale)
}

/// Launch a game via `steam://rungameid/<id>` so the user's per-game launch
/// options (the VR wrapper) are honoured. Steam apps: id == appid. Non-Steam
/// shortcuts: the 64-bit game id `(appid << 32) | 0x02000000`.
fn launch_game(g: &games::LoadedGame) {
    let game_id = if let Some(id) = &g.app_id {
        id.clone()
    } else if let Some(sid) = g.shortcut_id.as_ref().and_then(|s| s.parse::<u64>().ok()) {
        ((sid << 32) | 0x0200_0000).to_string()
    } else {
        log::warn!("'{}' has no launch id — can't launch it", g.name);
        return;
    };
    let uri = format!("steam://rungameid/{game_id}");
    log::info!("launching '{}' via {}", g.name, uri);
    let spawn = |bin: &str| {
        Command::new(bin).arg(&uri).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()
    };
    if spawn("steam").is_err() {
        if let Err(e) = spawn("xdg-open") {
            log::warn!("failed to launch '{}': {e}", g.name);
        }
    }
}

/// Create an overlay-flavoured `XrSession` by hand (the safe `openxr` wrapper
/// doesn't expose `XrSessionCreateInfoOverlayEXTX`).
unsafe fn create_overlay_session(
    instance: &xr::Instance,
    system: xr::SystemId,
    info: &xr::vulkan::SessionCreateInfo,
) -> std::result::Result<xr::sys::Session, xr::sys::Result> {
    use xr::sys::Handle;
    let overlay = xr::sys::SessionCreateInfoOverlayEXTX {
        ty: xr::sys::SessionCreateInfoOverlayEXTX::TYPE,
        next: std::ptr::null(),
        create_flags: xr::OverlaySessionCreateFlagsEXTX::EMPTY,
        session_layers_placement: 5,
    };
    let binding = xr::sys::GraphicsBindingVulkanKHR {
        ty: xr::sys::GraphicsBindingVulkanKHR::TYPE,
        next: (&raw const overlay).cast(),
        instance: info.instance,
        physical_device: info.physical_device,
        device: info.device,
        queue_family_index: info.queue_family_index,
        queue_index: info.queue_index,
    };
    let create_info = xr::sys::SessionCreateInfo {
        ty: xr::sys::SessionCreateInfo::TYPE,
        next: (&raw const binding).cast(),
        create_flags: xr::SessionCreateFlags::default(),
        system_id: system,
    };
    let mut out = xr::sys::Session::NULL;
    let r = (instance.fp().create_session)(instance.as_raw(), &raw const create_info, &raw mut out);
    if r.into_raw() >= 0 {
        Ok(out)
    } else {
        Err(r)
    }
}
