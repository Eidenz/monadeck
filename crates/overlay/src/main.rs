// monadeck-overlay — in-headset VR game-library launcher for Monado.
//
// An OpenXR overlay session (XR_EXTX_overlay) that renders one large
// SteamVR-dashboard-style egui panel as a composition-layer quad, with a
// controller laser pointer for selection and grip-to-move. Game discovery and
// cover art come from monadeck-core (shared with the desktop launcher).
//
// The OpenXR/Vulkan/egui/laser plumbing is adapted from monado-frame.

mod audio;
mod games;
mod gfx;
mod mathx;
mod monado;
mod ui;

use std::collections::{HashMap, HashSet};
use std::os::raw::c_char;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use anyhow::{bail, Result};
use ash::vk;
use ash::vk::Handle as _;
use openxr as xr;

use gfx::{
    cyl_layout, cylinder_layer, fill_laser, laser_quad, make_laser, make_panel, quad_layer,
    render_panel,
};
use mathx::{front_pose, locate_pose, offset_pose, pose_compose, pose_invert, posef, raycast, raycast_cylinder};

static VK_ENTRY: OnceLock<ash::Entry> = OnceLock::new();

const GRAB_START: f32 = 0.40; // grip FORCE to start moving the panel
const GRAB_RELEASE: f32 = 0.15;
/// Curve radius for the cylinder panel (m). Equals the panel's anchor distance,
/// so the cylinder axis lands at the viewer — a SteamVR-style wrap.
const CURVE_RADIUS: f32 = 1.5;

#[derive(Clone, Copy, PartialEq)]
enum PanelId {
    Main,
    Rail,
    Bottom,
}

/// The closest panel the laser hit this frame (one active pointer at a time).
#[derive(Clone, Copy)]
struct Hit {
    panel: PanelId,
    u: f32,
    v: f32,
    t: f32,
    down: bool,
    aim: xr::Posef,
    path: xr::Path,
}

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

    // Panel sizes (metres). Heights are derived from the texture aspect so the
    // cylinder hit-test matches what's rendered. Tune these for feel.
    const MAIN_W: f32 = 1.30;
    const RAIL_W: f32 = 0.105; // thinner rail (height preserved via px aspect)
    const BOTTOM_W: f32 = 1.30; // full main width
    const MAIN_H: f32 = MAIN_W * 1250.0 / 2000.0;
    const RAIL_H: f32 = RAIL_W * 1140.0 / 162.0;
    const BOTTOM_H: f32 = BOTTOM_W * 151.0 / 1640.0; // slimmer bar (width preserved)
    const PANEL_FWD: f32 = 0.06; // float rail/bottom this far in front of main
    const GAP: f32 = 0.05;
    // Curved placement: yaw the rail left of the main arc; drop the bar below.
    const RAIL_YAW: f32 = MAIN_W / CURVE_RADIUS / 2.0 + GAP + RAIL_W / (CURVE_RADIUS - PANEL_FWD) / 2.0;
    const BOTTOM_YOFF: f32 = -(MAIN_H / 2.0 + GAP + BOTTOM_H / 2.0);
    // Flat fallback offsets (runtimes without cylinder layers).
    const RAIL_DX: f32 = -(MAIN_W / 2.0 + GAP + RAIL_W / 2.0);
    const BOTTOM_DY: f32 = BOTTOM_YOFF;

    // The layout anchor (flat centre, faces the viewer). The three panels are
    // cylinder segments sharing one axis (curved together), positioned from the
    // anchor — so grabbing/recentring moves the whole set.
    let mut anchor = posef([0.0, 0.0, -CURVE_RADIUS]);
    let mut main_panel = make_panel(
        &session, &device, allocator.clone(), render_pass, format, srgb,
        (2000, 1250), (MAIN_W, MAIN_H), anchor,
    )?;
    let mut rail_panel = make_panel(
        &session, &device, allocator.clone(), render_pass, format, srgb,
        (162, 1140), (RAIL_W, RAIL_H), anchor,
    )?;
    let mut bottom_panel = make_panel(
        &session, &device, allocator.clone(), render_pass, format, srgb,
        (1640, 151), (BOTTOM_W, BOTTOM_H), anchor,
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
    let haptic_action = action_set.create_action::<xr::Haptic>("haptic", "Haptic tick", &[left_path, right_path])?;
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
            // Summon/dismiss is the LEFT system (menu) click only.
            xr::Binding::new(&system_action, xr_instance.string_to_path("/user/hand/left/input/system/click")?),
            xr::Binding::new(&haptic_action, xr_instance.string_to_path("/user/hand/left/output/haptic")?),
            xr::Binding::new(&haptic_action, xr_instance.string_to_path("/user/hand/right/output/haptic")?),
        ],
    )?;
    session.attach_action_sets(&[&action_set])?;
    let aim_left = aim_action.create_space(&session, left_path, xr::Posef::IDENTITY)?;
    let aim_right = aim_action.create_space(&session, right_path, xr::Posef::IDENTITY)?;

    // --- Game scan (background, metadata only) + lazy art decoder pool ------
    let scan_rx = games::spawn_scan();
    let art = games::ArtLoader::new();
    // libmonado link: running-game detection, recenter, input arbitration.
    let monado = monado::MonadoLink::new();
    let ov_cfg = monadeck_core::overlay_config::OverlayConfig::load();
    let mut audio = audio::Audio::new(ov_cfg.audio_enabled, ov_cfg.audio_volume);
    let mut audio_prev = (ov_cfg.audio_enabled, ov_cfg.audio_volume);
    let mut favorites: HashSet<String> = monadeck_core::favorites::load();

    let mut st = ui::LibState::new();
    st.audio_enabled = ov_cfg.audio_enabled;
    st.audio_volume = ov_cfg.audio_volume;

    // --- Loop state ---------------------------------------------------------
    let mut events = xr::EventDataBuffer::new();
    let mut running = false;
    let mut focused = false;
    let mut recenter = true;
    let mut visible = false; // default off — summon with a left system click
    let mut sys_prev = false;
    let mut sys_active_prev = false; // system action active-state edge
    let mut last_active_change: Option<Instant> = None; // when is_active last flipped
    let mut blocked_prev = false; // game-input arbitration edge state
    // (hand index, controller->panel offset) while grabbing.
    let mut grab: Option<(usize, xr::Posef)> = None;
    let start = Instant::now(); // egui clock (animations)
    let mut summon_at: Option<Instant> = None; // summon fade-in
    let mut launching_until: Option<Instant> = None; // "Launching…" hold before hide
    let mut click_prev = false; // haptic click edge
    let mut hover_prev: Option<usize> = None; // haptic hover edge
    // Re-scan to refresh last-played ordering when a game starts/stops.
    let mut running_app_prev: Option<String> = None;
    let mut refresh_rx: Option<std::sync::mpsc::Receiver<Vec<monadeck_core::steam::LibraryGame>>> = None;
    // Lazy-art LRU: per-slot last-used frame, to evict the coldest past a cap.
    let mut frame: u64 = 0;
    let mut last_used: HashMap<(usize, games::ArtKind), u64> = HashMap::new();

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

        // Drain the finished scan (metadata only; art loads lazily).
        if let Ok(rows) = scan_rx.try_recv() {
            st.games = games::to_games(rows);
            for g in st.games.iter_mut() {
                g.is_favorite = g.cover_id.as_ref().is_some_and(|id| favorites.contains(id));
            }
            st.scanning = false;
            if st.selected.is_none() && !st.games.is_empty() {
                st.selected = Some(0);
            }
        }

        // A game started or stopped -> last-played changed -> refresh the order.
        let running = monado.running_app();
        if running != running_app_prev {
            running_app_prev = running.clone();
            if refresh_rx.is_none() {
                refresh_rx = Some(games::spawn_scan());
            }
        }

        // --- Sync actions + summon/dismiss (left system click, rising edge) --
        if focused {
            session.sync_actions(&[(&action_set).into()])?;
            let sl = system_action.state(&session, left_path)?;
            let sys_active = sl.is_active;
            let sys_down = sl.is_active && sl.current_state;
            // Another overlay (e.g. WayVR) blocking/unblocking our input as your
            // cursor enters/leaves its window flips the action's `is_active`, which
            // can fake a system press — so ignore press edges for a moment around
            // any active-state flip (same fix monado-frame uses).
            if sys_active != sys_active_prev {
                sys_active_prev = sys_active;
                last_active_change = Some(Instant::now());
            }
            let settled = last_active_change.map_or(true, |t| t.elapsed().as_millis() > 150);
            if sys_down && !sys_prev && settled {
                visible = !visible;
                if visible {
                    recenter = true; // reappear in front of the head
                    summon_at = Some(Instant::now());
                    // Auto-select the running game, if any (SteamVR-style).
                    if let Some(app) = &running {
                        if let Some(i) = st.games.iter().position(|g| name_matches(&g.name, app)) {
                            st.selected = Some(i);
                        }
                    }
                } else {
                    launching_until = None; // dismissed; drop any launch overlay
                    st.launching_name = None;
                }
            }
            sys_prev = sys_down;
        }

        // "Launching…" hold expired → hide the dashboard.
        if let Some(until) = launching_until {
            if Instant::now() >= until {
                launching_until = None;
                st.launching_name = None;
                visible = false;
            }
        }

        // Hidden: apply any finished refresh (rebuild while out of sight, so the
        // order is fresh on the next summon), drop input block, skip rendering.
        if !visible {
            if let Some(rx) = &refresh_rx {
                if let Ok(rows) = rx.try_recv() {
                    st.games = games::to_games(rows);
                    for g in st.games.iter_mut() {
                        g.is_favorite = g.cover_id.as_ref().is_some_and(|id| favorites.contains(id));
                    }
                    st.selected = (!st.games.is_empty()).then_some(0);
                    last_used.clear();
                    refresh_rx = None;
                }
            }
            if blocked_prev {
                monado.set_block(false);
                blocked_prev = false;
            }
            frame_stream.end(time, blend_mode, &[])?;
            continue;
        }

        // Reflect the running game into the UI each frame.
        st.running_index = running.as_ref().and_then(|app| st.games.iter().position(|g| name_matches(&g.name, app)));

        // Place the layout in front of the head on first show / on recenter.
        if recenter {
            if let Some(h) = hmd {
                anchor = front_pose(&h, CURVE_RADIUS, 0.0, 0.0);
                recenter = false;
            }
        }
        // Flat poses (used by the quad fallback + render).
        main_panel.pose = anchor;
        rail_panel.pose = offset_pose(&anchor, RAIL_DX, 0.0, PANEL_FWD);
        bottom_panel.pose = offset_pose(&anchor, 0.0, BOTTOM_DY, PANEL_FWD);
        // Curved placements on the shared cylinder (hit-test + layers).
        let main_l = cyl_layout(&anchor, CURVE_RADIUS, CURVE_RADIUS, 0.0, 0.0, MAIN_W, MAIN_H);
        let rail_l = cyl_layout(&anchor, CURVE_RADIUS, CURVE_RADIUS - PANEL_FWD, RAIL_YAW, 0.0, RAIL_W, RAIL_H);
        let bottom_l = cyl_layout(&anchor, CURVE_RADIUS, CURVE_RADIUS - PANEL_FWD, 0.0, BOTTOM_YOFF, BOTTOM_W, BOTTOM_H);

        // Bottom-bar live data.
        st.clock = chrono::Local::now().format("%-I:%M %p").to_string();
        st.batteries = monado.batteries();

        // Summon fade-in (1 -> 0 over ~0.22 s).
        st.fade_in = summon_at.map_or(0.0, |t| (1.0 - t.elapsed().as_secs_f32() / 0.22).clamp(0.0, 1.0));

        // --- Input: laser hit-test across the 3 panels + grip-to-move --------
        let mut best: Option<Hit> = None;
        let mut scroll = (0.0f32, 0.0f32);
        if focused {
            // Continue an in-progress grab — moves the whole layout anchor.
            if let Some((hand_i, offset)) = grab {
                let (path, aim) = if hand_i == 0 { (left_path, &aim_left) } else { (right_path, &aim_right) };
                let grip = grab_action.state(&session, path)?.current_state;
                if grip < GRAB_RELEASE {
                    grab = None;
                } else if let Some(p) = locate_pose(aim, &space, time) {
                    anchor = pose_compose(&p, &offset);
                }
            }

            if grab.is_none() {
                for (idx, (aim, path)) in [(&aim_left, left_path), (&aim_right, right_path)].into_iter().enumerate() {
                    let Some(p) = locate_pose(aim, &space, time) else { continue };
                    let candidates = if curved {
                        [
                            (PanelId::Main, raycast_cylinder(&p, &main_l.pose, main_l.radius, main_l.central_angle, main_l.height)),
                            (PanelId::Rail, raycast_cylinder(&p, &rail_l.pose, rail_l.radius, rail_l.central_angle, rail_l.height)),
                            (PanelId::Bottom, raycast_cylinder(&p, &bottom_l.pose, bottom_l.radius, bottom_l.central_angle, bottom_l.height)),
                        ]
                    } else {
                        [
                            (PanelId::Main, raycast(&p, &main_panel.pose, main_panel.size_m)),
                            (PanelId::Rail, raycast(&p, &rail_panel.pose, rail_panel.size_m)),
                            (PanelId::Bottom, raycast(&p, &bottom_panel.pose, bottom_panel.size_m)),
                        ]
                    };
                    let pointing = candidates.iter().any(|(_, h)| h.is_some());
                    // Grip while pointing at any panel grabs the whole layout.
                    let grip = grab_action.state(&session, path)?.current_state;
                    if grip > GRAB_START && pointing {
                        grab = Some((idx, pose_compose(&pose_invert(&p), &anchor)));
                        best = None;
                        break;
                    }
                    let down = select_action.state(&session, path)?.current_state > 0.5;
                    for (panel, hit) in candidates {
                        if let Some((u, v, t)) = hit {
                            if best.map_or(true, |b| t < b.t) {
                                best = Some(Hit { panel, u, v, t, down, aim: p, path });
                            }
                        }
                    }
                }
                // Thumbstick scrolls only when pointing at the main panel.
                if let Some(h) = best {
                    if h.panel == PanelId::Main {
                        let s = scroll_action.state(&session, h.path)?.current_state;
                        scroll = deadzone(s.x, s.y);
                    }
                }
            }
        }

        let main_ptr = best.filter(|h| h.panel == PanelId::Main).map(|h| (h.u, h.v, h.down));
        let rail_ptr = best.filter(|h| h.panel == PanelId::Rail).map(|h| (h.u, h.v, h.down));
        let bottom_ptr = best.filter(|h| h.panel == PanelId::Bottom).map(|h| (h.u, h.v, h.down));
        let laser_ray = best.map(|h| (h.aim, h.t));

        // Block the game's controller input while pointing at the dashboard.
        let want_block = best.is_some();
        if want_block != blocked_prev {
            monado.set_block(want_block);
            blocked_prev = want_block;
        }

        // --- Render the three panels ----------------------------------------
        let elapsed = start.elapsed().as_secs_f64();
        render_panel(&mut main_panel, &device, render_pass, cmd, cmd_pool, queue, fence, false, main_ptr, scroll, elapsed, |ctx| {
            ui::build_main(ctx, &mut st)
        })?;
        render_panel(&mut rail_panel, &device, render_pass, cmd, cmd_pool, queue, fence, true, rail_ptr, (0.0, 0.0), elapsed, |ctx| {
            ui::build_rail(ctx, &mut st)
        })?;
        render_panel(&mut bottom_panel, &device, render_pass, cmd, cmd_pool, queue, fence, true, bottom_ptr, (0.0, 0.0), elapsed, |ctx| {
            ui::build_bottom(ctx, &mut st)
        })?;

        // --- Lazy art: upload finished decodes, request on-screen/selected --
        while let Some(res) = art.try_recv() {
            if let Some(g) = st.games.get_mut(res.index) {
                let key = format!("art-{}-{:?}", res.index, res.kind);
                *g.art_mut(res.kind) = match res.image {
                    Some(img) => games::ArtState::Ready(main_panel.ctx.load_texture(
                        key,
                        img,
                        egui::TextureOptions::LINEAR,
                    )),
                    None => games::ArtState::Missing,
                };
            }
        }
        let mut wants: Vec<(usize, games::ArtKind)> =
            st.visible_now.iter().map(|&i| (i, games::ArtKind::Cover)).collect();
        if let Some(i) = st.selected {
            wants.push((i, games::ArtKind::Cover));
            wants.push((i, games::ArtKind::Hero));
            wants.push((i, games::ArtKind::Logo));
        }
        if st.show_splash {
            if let Some(i) = st.running_index {
                wants.push((i, games::ArtKind::Cover));
                wants.push((i, games::ArtKind::Hero));
            }
        }
        for (i, kind) in wants {
            want_art(&mut st.games, i, kind, &art);
        }

        // --- Lazy-art LRU: evict the coldest textures past a cap ------------
        frame += 1;
        let mut used: HashSet<(usize, games::ArtKind)> =
            st.visible_now.iter().map(|&i| (i, games::ArtKind::Cover)).collect();
        if let Some(i) = st.selected {
            for k in games::ART_KINDS {
                used.insert((i, k));
            }
        }
        for key in &used {
            last_used.insert(*key, frame);
        }
        const ART_CAP: usize = 160;
        let mut ready: Vec<(u64, usize, games::ArtKind)> = Vec::new();
        for (gi, g) in st.games.iter().enumerate() {
            for k in games::ART_KINDS {
                if matches!(g.art(k), games::ArtState::Ready(_)) {
                    ready.push((*last_used.get(&(gi, k)).unwrap_or(&0), gi, k));
                }
            }
        }
        if ready.len() > ART_CAP {
            ready.sort_by_key(|&(lu, _, _)| lu); // coldest first
            for &(_, gi, k) in ready.iter().take(ready.len() - ART_CAP) {
                if !used.contains(&(gi, k)) {
                    *st.games[gi].art_mut(k) = games::ArtState::Idle;
                    last_used.remove(&(gi, k));
                }
            }
        }

        // --- Haptics: firm tick on click, light tick on hovering a new game --
        if let Some(h) = best {
            if h.down && !click_prev {
                pulse(&session, &haptic_action, h.path, 0.5, 28);
            }
            click_prev = h.down;
            if st.hovered_index.is_some() && st.hovered_index != hover_prev {
                pulse(&session, &haptic_action, h.path, 0.16, 9);
            }
        } else {
            click_prev = false;
        }
        hover_prev = st.hovered_index;

        if laser_ray.is_some() {
            fill_laser(&mut laser, &device, cmd, queue, fence)?;
        }

        // All three panels as curved cylinder segments (rail + bottom alpha so
        // they float as rounded cards); quad fallback if no cylinder support.
        // Declared out here so each layer outlives the pointer vec.
        let (main_cyl, rail_cyl, bottom_cyl);
        let (main_quad, rail_quad, bottom_quad);
        let laser_q = match (laser_ray, hmd) {
            (Some((aim, t)), Some(h)) => Some(laser_quad(&laser, &space, &aim, t, &h)),
            _ => None,
        };
        let mut layers: Vec<&xr::CompositionLayerBase<xr::Vulkan>> = Vec::new();
        if curved {
            main_cyl = cylinder_layer(&main_panel, &space, &main_l, false);
            rail_cyl = cylinder_layer(&rail_panel, &space, &rail_l, true);
            bottom_cyl = cylinder_layer(&bottom_panel, &space, &bottom_l, true);
            layers.push(&main_cyl);
            layers.push(&rail_cyl);
            layers.push(&bottom_cyl);
        } else {
            main_quad = quad_layer(&main_panel, &space, false);
            rail_quad = quad_layer(&rail_panel, &space, true);
            bottom_quad = quad_layer(&bottom_panel, &space, true);
            layers.push(&main_quad);
            layers.push(&rail_quad);
            layers.push(&bottom_quad);
        }
        if let Some(q) = &laser_q {
            layers.push(q);
        }
        frame_stream.end(time, blend_mode, &layers)?;

        // --- Drain UI actions -----------------------------------------------
        if let Some(i) = st.launch_request.take() {
            if let Some(g) = st.games.get(i) {
                launch_game(g);
                audio.launch();
                st.launching_name = Some(g.name.clone());
                // Hold the "Launching…" overlay briefly, then auto-hide.
                launching_until = Some(Instant::now() + std::time::Duration::from_millis(1500));
            }
            // Optimistically mark it "now" so it's at the top when you return
            // (a real re-scan confirms it when the game starts/stops).
            if let Some(g) = st.games.get_mut(i) {
                g.last_played = Some(now_unix());
            }
            resort_recency(&mut st);
            last_used.clear();
        }
        if st.sound_select {
            st.sound_select = false;
            audio.select();
        }
        if st.sound_tab {
            st.sound_tab = false;
            audio.tab();
        }
        // Sound settings changed in the Settings tab — apply live + persist.
        if (st.audio_enabled, st.audio_volume) != audio_prev {
            audio.set_enabled(st.audio_enabled);
            audio.set_volume(st.audio_volume);
            audio_prev = (st.audio_enabled, st.audio_volume);
            monadeck_core::overlay_config::OverlayConfig {
                audio_enabled: st.audio_enabled,
                audio_volume: st.audio_volume,
            }
            .save();
        }
        if st.stop_request.take().is_some() {
            if let Some(app) = monado.running_app() {
                stop_game(&app);
            }
        }
        if let Some(i) = st.favorite_toggle_request.take() {
            if let Some(g) = st.games.get_mut(i) {
                g.is_favorite = !g.is_favorite;
                if let Some(id) = &g.cover_id {
                    if g.is_favorite {
                        favorites.insert(id.clone());
                    } else {
                        favorites.remove(id);
                    }
                }
                monadeck_core::favorites::save(&favorites);
            }
        }
        if st.recenter_request {
            st.recenter_request = false;
            recenter = true;
        }
        if st.recenter_playspace_request {
            st.recenter_playspace_request = false;
            monado.recenter();
        }
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Re-sort the catalogue most-recently-played first (art moves with each entry,
/// so nothing reloads), keeping the same game selected by identity.
fn resort_recency(st: &mut ui::LibState) {
    let sel_id = st.selected.and_then(|i| st.games.get(i)).and_then(|g| g.cover_id.clone());
    st.games.sort_by(|a, b| {
        b.last_played
            .unwrap_or(0)
            .cmp(&a.last_played.unwrap_or(0))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    st.selected = sel_id.and_then(|id| st.games.iter().position(|g| g.cover_id.as_deref() == Some(id.as_str())));
}

/// Fire a haptic tick on a controller (`amplitude` 0..1, `millis` duration).
fn pulse(
    session: &xr::Session<xr::Vulkan>,
    haptic: &xr::Action<xr::Haptic>,
    hand: xr::Path,
    amplitude: f32,
    millis: u64,
) {
    let v = xr::HapticVibration::new()
        .amplitude(amplitude)
        .frequency(0.0)
        .duration(xr::Duration::from_nanos((millis * 1_000_000) as i64));
    let _ = haptic.apply_feedback(session, hand, &v);
}

/// Whether a catalogue game name and a libmonado client (OpenXR app) name refer
/// to the same game — loose, since the two don't always match exactly.
fn name_matches(game: &str, app: &str) -> bool {
    let (g, a) = (game.to_lowercase(), app.to_lowercase());
    !g.is_empty() && !a.is_empty() && (g == a || g.contains(&a) || a.contains(&g))
}

/// Best-effort "stop the running game". libmonado has no kill API, so we SIGTERM
/// processes whose command line matches the app's (sanitised) name. Imperfect —
/// a clean version needs the client PID exposed by libmonado.
fn stop_game(app: &str) {
    let name: String = app
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '.' | '_' | '-'))
        .collect();
    let name = name.trim();
    if name.len() < 3 {
        log::warn!("stop: app name '{app}' too short to match safely");
        return;
    }
    log::info!("stop: SIGTERM processes matching '{name}'");
    let _ = Command::new("pkill")
        .arg("-TERM")
        .arg("-f")
        .arg(name)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

/// Request art for one slot if it hasn't been requested yet (Idle -> Pending),
/// or mark it Missing when the game has no art id.
fn want_art(games: &mut [games::LibGame], i: usize, kind: games::ArtKind, loader: &games::ArtLoader) {
    if !games[i].art(kind).is_idle() {
        return;
    }
    let cover_id = games[i].cover_id.clone();
    let slot = games[i].art_mut(kind);
    match cover_id {
        Some(id) => {
            *slot = games::ArtState::Pending;
            loader.request(i, kind, id);
        }
        None => *slot = games::ArtState::Missing,
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
fn launch_game(g: &games::LibGame) {
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
        // The compositor stacks overlay sessions by increasing placement. WayVR
        // (and monado-frame/nemurixr) use 5; sit above WayVR's background image so
        // the dashboard isn't hidden underneath it.
        session_layers_placement: 12,
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
