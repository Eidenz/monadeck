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

/// An active notification: shown on its own floating layer (over a game too),
/// auto-dismissed at `until`.
struct ToastState {
    title: String,
    body: String,
    kind: ui::ToastKind,
    pose: xr::Posef,
    until: Instant,
}

/// A toast tucked into the lower view (~1.3 m ahead, dropped below the gaze) so
/// it's read at a glance without blocking what you're looking at.
fn make_toast(
    title: impl Into<String>,
    body: impl Into<String>,
    kind: ui::ToastKind,
    hmd: &xr::Posef,
) -> ToastState {
    ToastState {
        title: title.into(),
        body: body.into(),
        kind,
        pose: mathx::toast_pose(hmd, 1.3, 0.42),
        until: Instant::now() + std::time::Duration::from_secs(5),
    }
}

/// Stamp the per-user metadata (favorite flag · tracked playtime · collection
/// membership) onto a freshly scanned catalogue, keyed by each game's cover id.
fn apply_user_meta(
    games: &mut [games::LibGame],
    favorites: &HashSet<String>,
    uevr_games: &HashSet<String>,
    playtime: &HashMap<String, u64>,
    collections: &[monadeck_core::collections::Collection],
) {
    for g in games.iter_mut() {
        match g.cover_id.as_ref() {
            Some(id) => {
                g.is_favorite = favorites.contains(id);
                g.uevr = uevr_games.contains(id);
                g.tracked_minutes = playtime.get(id).map(|&s| (s / 60) as u32).filter(|&m| m > 0);
                g.collections = collections
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.members.iter().any(|m| m == id))
                    .map(|(ci, _)| ci)
                    .collect();
            }
            None => {
                g.is_favorite = false;
                g.uevr = false;
                g.tracked_minutes = None;
                g.collections.clear();
            }
        }
    }
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
    // The rail yaw / bottom drop / flat-fallback offsets are derived per-frame
    // from the live comfort knobs (distance · size · curve), down in the loop.

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
    // A small notification panel (timer alarm, low battery) — shows independently
    // of the dashboard, even over a running game.
    let mut toast_panel = make_panel(
        &session, &device, allocator.clone(), render_pass, format, srgb,
        (720, 168), (0.44, 0.44 * 168.0 / 720.0), anchor,
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
    let mut settings_prev = (
        ov_cfg.audio_enabled,
        ov_cfg.audio_volume,
        ov_cfg.summon_tilt,
        ov_cfg.panel_dist,
        ov_cfg.panel_scale,
        ov_cfg.panel_curve,
        ov_cfg.playspace_x,
        ov_cfg.playspace_y,
        ov_cfg.playspace_z,
        ov_cfg.playspace_yaw,
        ov_cfg.uevr_delay,
    );
    let mut favorites: HashSet<String> = monadeck_core::favorites::load();
    // Games the user flagged to launch through UEVR ("VR Mod").
    let mut uevr_games: HashSet<String> = monadeck_core::uevr::load_enabled();
    // Re-place the dashboard in front of you when the distance knob changes.
    let mut panel_dist_prev = ov_cfg.panel_dist;
    // Playspace offset: apply the persisted GLOBAL offset now so it lands as soon
    // as the service connects. `applied_ps_prev` then tracks the *effective* offset
    // in the loop — per-game overrides (below) take over while a game is running.
    let mut applied_ps_prev =
        (ov_cfg.playspace_x, ov_cfg.playspace_y, ov_cfg.playspace_z, ov_cfg.playspace_yaw);
    if applied_ps_prev != (0.0, 0.0, 0.0, 0.0) {
        monado.set_origin(
            ov_cfg.playspace_x,
            ov_cfg.playspace_y,
            ov_cfg.playspace_z,
            ov_cfg.playspace_yaw.to_radians(),
        );
    }
    // Per-game playspace overrides (keyed by cover id) + which game is running.
    let mut ps_overrides: HashMap<String, [f32; 4]> = monadeck_core::playspace_overrides::load();
    let mut running_cover_prev: Option<String> = None;
    // Tracked playtime: total seconds per game key + the in-progress session.
    let mut playtime: HashMap<String, u64> = monadeck_core::playtime::load();
    let mut session_start: Option<Instant> = None;
    let mut session_key: Option<String> = None;
    // User collections (named groups of games).
    let mut collections = monadeck_core::collections::load();

    let mut st = ui::LibState::new();
    st.audio_enabled = ov_cfg.audio_enabled;
    st.audio_volume = ov_cfg.audio_volume;
    st.summon_tilt = ov_cfg.summon_tilt;
    st.panel_dist = ov_cfg.panel_dist;
    st.panel_scale = ov_cfg.panel_scale;
    st.panel_curve = ov_cfg.panel_curve;
    st.playspace_x = ov_cfg.playspace_x;
    st.playspace_y = ov_cfg.playspace_y;
    st.playspace_z = ov_cfg.playspace_z;
    st.playspace_yaw = ov_cfg.playspace_yaw;
    st.uevr_delay = ov_cfg.uevr_delay;
    st.freeze_delay_secs = ov_cfg.freeze_delay_secs;
    // Hide the UEVR feature entirely if protontricks-launch isn't installed.
    st.uevr_available = monadeck_core::uevr::protontricks_available();
    // If protontricks is present, make sure the chihuahua injector is too —
    // download it on first run, in the background, so it's ready before the user
    // launches a VR-Mod game. (No-op when it's already present.)
    if st.uevr_available {
        std::thread::spawn(|| match monadeck_core::uevr::ensure_chihuahua() {
            Ok(p) => log::info!("UEVR injector ready: {}", p.display()),
            Err(e) => log::warn!("UEVR injector unavailable (download/locate failed): {e}"),
        });
    }
    st.collections = collections.iter().map(|c| c.name.clone()).collect();

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
    // For a UEVR launch: keep the dashboard up showing "Waiting for UEVR injection…"
    // until the game appears as a VR client (= injected) or this deadline passes.
    let mut uevr_wait: Option<Instant> = None;
    // A freeze counting down before it applies: (client id, deadline).
    let mut pending_freeze: Option<(u32, Instant)> = None;
    let mut click_prev = false; // haptic click edge
    let mut hover_prev: Option<usize> = None; // haptic hover edge
    // Re-scan to refresh last-played ordering when a game starts/stops.
    let mut running_app_prev: Option<String> = None;
    let mut refresh_rx: Option<std::sync::mpsc::Receiver<Vec<monadeck_core::steam::LibraryGame>>> = None;
    let mut manual_refresh = false; // a user-requested "Refresh library" is pending
    // Notifications + timer (run even while the dashboard is hidden).
    let mut toast: Option<ToastState> = None;
    let mut timer_end: Option<Instant> = None;
    let mut timer_paused: Option<u32> = None;
    let mut battery_low_warned = false;
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
            apply_user_meta(&mut st.games, &favorites, &uevr_games, &playtime, &collections);
            st.scanning = false;
            if st.selected.is_none() && !st.games.is_empty() {
                st.selected = Some(0);
            }
        }

        // Manual "Refresh library": apply the re-scan as soon as it lands, even
        // while the dashboard is visible (rebuild resets art to Idle, so covers
        // added at runtime get re-probed). Selection is preserved/clamped.
        if manual_refresh {
            if let Some(rx) = &refresh_rx {
                if let Ok(rows) = rx.try_recv() {
                    st.games = games::to_games(rows);
                    apply_user_meta(&mut st.games, &favorites, &uevr_games, &playtime, &collections);
                    if st.selected.map_or(false, |i| i >= st.games.len()) {
                        st.selected = (!st.games.is_empty()).then_some(0);
                    }
                    last_used.clear();
                    refresh_rx = None;
                    manual_refresh = false;
                }
            }
        }

        // A game started or stopped -> last-played changed -> refresh the order.
        let running = monado.running_app();
        if running != running_app_prev {
            // Close the previous session into the store (ignore <30 s blips).
            if let (Some(s), Some(key)) = (session_start.take(), session_key.take()) {
                let secs = s.elapsed().as_secs();
                if secs >= 30 {
                    *playtime.entry(key).or_insert(0) += secs;
                    monadeck_core::playtime::save(&playtime);
                }
            }
            // Open a new session for the now-running game (if we can key it).
            if let Some(app) = &running {
                if let Some(key) = st
                    .games
                    .iter()
                    .find(|g| name_matches(&g.name, app))
                    .and_then(|g| g.cover_id.clone())
                {
                    session_start = Some(Instant::now());
                    session_key = Some(key);
                }
            }
            running_app_prev = running.clone();
            if refresh_rx.is_none() {
                refresh_rx = Some(games::spawn_scan());
            }
        }
        // UEVR launch in progress: auto-close the dashboard once the game appears
        // as a VR client (= UEVR injected) or the wait times out.
        if let Some(timeout) = uevr_wait {
            if running.is_some() || Instant::now() >= timeout {
                uevr_wait = None;
                launching_until = None;
                st.launching_name = None;
                st.launching_status = None;
                visible = false;
            }
        }
        // Live "this session" minutes for the splash.
        st.session_minutes = session_start.map(|s| (s.elapsed().as_secs() / 60) as u32);

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
                    uevr_wait = None;
                    st.launching_name = None;
                    st.launching_status = None;
                }
            }
            sys_prev = sys_down;
        }

        // "Launching…" hold expired → hide the dashboard (normal game), or for a
        // UEVR game swap to the injection-wait message and stay open.
        if let Some(until) = launching_until {
            if Instant::now() >= until {
                launching_until = None;
                if uevr_wait.is_some() {
                    st.launching_status = Some("Waiting for UEVR injection…".into());
                } else {
                    st.launching_name = None;
                    st.launching_status = None;
                    visible = false;
                }
            }
        }

        // --- Timer, low-battery warning, toasts (run even while hidden) ------
        let now = Instant::now();
        st.batteries = monado.batteries();
        st.monado_clients = monado.clients();
        st.monado_freeze_supported = monado.freeze_supported();

        // Timer: start / pause / resume / reset, then count down + fire.
        if st.timer_toggle_request {
            st.timer_toggle_request = false;
            if let Some(end) = timer_end.take() {
                timer_paused = Some(end.saturating_duration_since(now).as_secs() as u32);
            } else if let Some(p) = timer_paused.take() {
                timer_end = Some(now + std::time::Duration::from_secs(p as u64));
            } else if st.timer_secs > 0 {
                timer_end = Some(now + std::time::Duration::from_secs(st.timer_secs as u64));
                st.timer_total = st.timer_secs;
            }
        }
        if st.timer_reset_request {
            st.timer_reset_request = false;
            timer_end = None;
            timer_paused = None;
        }
        if let Some(end) = timer_end {
            let rem = end.saturating_duration_since(now).as_secs() as u32;
            if rem == 0 {
                timer_end = None;
                if let Some(h) = hmd {
                    toast = Some(make_toast("Timer finished", "", ui::ToastKind::Timer, &h));
                }
                audio.alarm();
                pulse(&session, &haptic_action, left_path, 0.7, 60);
                pulse(&session, &haptic_action, right_path, 0.7, 60);
                st.timer_remaining = 0;
                st.timer_running = false;
                st.timer_paused = false;
            } else {
                st.timer_remaining = rem;
                st.timer_running = true;
                st.timer_paused = false;
            }
        } else if let Some(p) = timer_paused {
            st.timer_remaining = p;
            st.timer_running = false;
            st.timer_paused = true;
        } else {
            st.timer_remaining = st.timer_secs;
            st.timer_running = false;
            st.timer_paused = false;
        }

        // Low-battery warning: once per low episode, reset (hysteresis) above 20%.
        if let Some(low) = st
            .batteries
            .iter()
            .filter(|b| !b.charging)
            .min_by(|a, b| a.charge.partial_cmp(&b.charge).unwrap_or(std::cmp::Ordering::Equal))
        {
            if low.charge < 0.15 && !battery_low_warned {
                battery_low_warned = true;
                if let Some(h) = hmd {
                    let kind = match low.kind {
                        monado::BatteryKind::Glove => "Glove",
                        monado::BatteryKind::Controller => "Controller",
                        monado::BatteryKind::Tracker => "Tracker",
                        monado::BatteryKind::Other => "Device",
                    };
                    toast = Some(make_toast(
                        "Low battery",
                        format!("{kind} at {}%", (low.charge * 100.0).round() as i32),
                        ui::ToastKind::Battery,
                        &h,
                    ));
                }
                audio.alarm();
            }
        }
        if !st.batteries.iter().any(|b| !b.charging && b.charge < 0.20) {
            battery_low_warned = false;
        }

        // Expire + render the toast on its own layer (shows even over a game).
        if toast.as_ref().map_or(false, |t| now >= t.until) {
            toast = None;
        }
        let toast_active = toast.is_some();
        if let Some(t) = &toast {
            toast_panel.pose = t.pose;
            render_panel(
                &mut toast_panel, &device, render_pass, cmd, cmd_pool, queue, fence,
                true, None, (0.0, 0.0), start.elapsed().as_secs_f64(),
                |ctx| ui::build_toast(ctx, &t.title, &t.body, t.kind),
            )?;
        }

        // --- Playspace: per-game override tracking + apply ------------------
        // Runs even while hidden, so a game's offset is active during play with the
        // dashboard dismissed. Track the running game by cover id; when it changes,
        // load its stored override into the editor's per-game buffer.
        let running_cover = running
            .as_ref()
            .and_then(|app| st.games.iter().find(|g| name_matches(&g.name, app)))
            .and_then(|g| g.cover_id.clone());
        if running_cover != running_cover_prev {
            running_cover_prev = running_cover.clone();
            match &running_cover {
                Some(cid) => {
                    st.ps_game_active = true;
                    st.ps_game_name = running
                        .as_ref()
                        .and_then(|app| st.games.iter().find(|g| name_matches(&g.name, app)))
                        .map(|g| g.name.clone())
                        .unwrap_or_default();
                    if let Some(o) = ps_overrides.get(cid) {
                        st.ps_game_override = true;
                        st.ps_target_game = true; // it already has one — edit it by default
                        (st.ps_game_x, st.ps_game_y, st.ps_game_z, st.ps_game_yaw) =
                            (o[0], o[1], o[2], o[3]);
                    } else {
                        st.ps_game_override = false;
                        (st.ps_game_x, st.ps_game_y, st.ps_game_z, st.ps_game_yaw) = (0.0, 0.0, 0.0, 0.0);
                    }
                }
                None => {
                    st.ps_game_active = false;
                    st.ps_target_game = false;
                    st.ps_game_override = false;
                }
            }
        }
        // Effective offset = the running game's override (if any) else the global.
        let eff_ps = if st.ps_game_active && st.ps_game_override {
            (st.ps_game_x, st.ps_game_y, st.ps_game_z, st.ps_game_yaw)
        } else {
            (st.playspace_x, st.playspace_y, st.playspace_z, st.playspace_yaw)
        };
        if eff_ps != applied_ps_prev {
            applied_ps_prev = eff_ps;
            monado.set_origin(eff_ps.0, eff_ps.1, eff_ps.2, eff_ps.3.to_radians());
        }

        // Hidden: apply any finished refresh (rebuild while out of sight, so the
        // order is fresh on the next summon), drop input block, render only toasts.
        if !visible {
            if let Some(rx) = &refresh_rx {
                if let Ok(rows) = rx.try_recv() {
                    st.games = games::to_games(rows);
                    apply_user_meta(&mut st.games, &favorites, &uevr_games, &playtime, &collections);
                    st.selected = (!st.games.is_empty()).then_some(0);
                    last_used.clear();
                    refresh_rx = None;
                }
            }
            if blocked_prev {
                monado.set_block(false);
                blocked_prev = false;
            }
            let toast_q;
            let mut layers: Vec<&xr::CompositionLayerBase<xr::Vulkan>> = Vec::new();
            if toast_active {
                toast_q = quad_layer(&toast_panel, &space, true);
                layers.push(&toast_q);
            }
            frame_stream.end(time, blend_mode, &layers)?;
            continue;
        }

        // Reflect the running game into the UI each frame.
        st.running_index = running.as_ref().and_then(|app| st.games.iter().position(|g| name_matches(&g.name, app)));

        // Comfort knobs (live): distance, size, curvature. The dashboard is a
        // head-centred cylinder, so distance = anchor placement, the cylinder
        // radius `r = dist * curve` (bigger = flatter while the surface stays at
        // `dist`), and `scale` multiplies every panel's metric size + gaps.
        let dist = st.panel_dist.clamp(0.7, 3.0);
        let scale = st.panel_scale.clamp(0.6, 1.6);
        let r = dist * st.panel_curve.clamp(1.0, 4.0);
        let main_w = MAIN_W * scale;
        let main_h = MAIN_H * scale;
        let rail_w = RAIL_W * scale;
        let rail_h = RAIL_H * scale;
        let bottom_w = BOTTOM_W * scale;
        let bottom_h = BOTTOM_H * scale;
        let gap_m = GAP * scale;
        // The rail sits a fixed *linear* gap left of the main arc; dividing by the
        // radius turns it into the right yaw angle, so flattening (bigger r) no
        // longer flings it sideways (GAP·r blow-up). `GAP*CURVE_RADIUS` keeps the
        // default look identical to the old fixed-angle gap.
        let rail_gap = GAP * CURVE_RADIUS * scale;
        let rail_yaw = main_w / (2.0 * r) + rail_gap / r + rail_w / (2.0 * (r - PANEL_FWD));
        let bottom_yoff = -(main_h / 2.0 + gap_m + bottom_h / 2.0);
        let rail_dx = -(main_w / 2.0 + gap_m + rail_w / 2.0);

        // Place the layout in front of the head on first show / on recenter.
        if recenter {
            if let Some(h) = hmd {
                anchor = front_pose(&h, dist, 0.0, 0.0, st.summon_tilt);
                recenter = false;
            }
        }
        // Flat poses (used by the quad fallback + render).
        main_panel.pose = anchor;
        main_panel.size_m = (main_w, main_h);
        rail_panel.pose = offset_pose(&anchor, rail_dx, 0.0, PANEL_FWD);
        rail_panel.size_m = (rail_w, rail_h);
        bottom_panel.pose = offset_pose(&anchor, 0.0, bottom_yoff, PANEL_FWD);
        bottom_panel.size_m = (bottom_w, bottom_h);
        // Curved placements on the shared cylinder (hit-test + layers).
        let main_l = cyl_layout(&anchor, r, r, 0.0, 0.0, main_w, main_h);
        let rail_l = cyl_layout(&anchor, r, r - PANEL_FWD, rail_yaw, 0.0, rail_w, rail_h);
        let bottom_l = cyl_layout(&anchor, r, r - PANEL_FWD, 0.0, bottom_yoff, bottom_w, bottom_h);

        // Bottom-bar clock (batteries were refreshed before the visibility gate).
        st.clock = chrono::Local::now().format("%-I:%M %p").to_string();

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
        let toast_q;
        if toast_active {
            toast_q = quad_layer(&toast_panel, &space, true);
            layers.push(&toast_q);
        }
        if let Some(q) = &laser_q {
            layers.push(q);
        }
        frame_stream.end(time, blend_mode, &layers)?;

        // --- Drain UI actions -----------------------------------------------
        if let Some(i) = st.launch_request.take() {
            let mut launched = false;
            if let Some(g) = st.games.get(i) {
                // UEVR injection needs a Proton prefix. A non-Steam shortcut only gets
                // one after it's been launched once through Steam (with Proton forced),
                // so without it protontricks silently no-ops. Detect + warn instead of
                // pretending to launch.
                let uevr_blocked = g.uevr && {
                    let appid = g.shortcut_id.as_deref().or(g.app_id.as_deref());
                    !appid.is_some_and(monadeck_core::steam::has_proton_prefix)
                };
                if uevr_blocked {
                    if let Some(h) = hmd {
                        toast = Some(make_toast(
                            "Launch it in Steam first",
                            format!(
                                "{} has no Proton prefix yet. Force Proton in its Steam properties and run \
                                 it once, then VR Mod will work.",
                                g.name
                            ),
                            ui::ToastKind::Info,
                            &h,
                        ));
                    }
                    st.sound_tab = true;
                } else {
                    if g.uevr {
                        launch_uevr(g, st.uevr_delay);
                        // Keep the dashboard up through injection; the wait ends when the
                        // game shows up as a VR client, or this deadline passes.
                        uevr_wait = Some(
                            Instant::now() + std::time::Duration::from_secs(st.uevr_delay as u64 + 90),
                        );
                    } else {
                        launch_game(g);
                        uevr_wait = None;
                    }
                    audio.launch();
                    st.launching_name = Some(g.name.clone());
                    st.launching_status = None;
                    // Hold the "Launching…" card briefly; a normal game then auto-hides,
                    // a UEVR game switches to the injection-wait message and stays open.
                    launching_until = Some(Instant::now() + std::time::Duration::from_millis(1500));
                    launched = true;
                }
            }
            // Optimistically mark it "now" so it's at the top when you return
            // (a real re-scan confirms it when the game starts/stops). Only when we
            // actually launched — a blocked UEVR warning shouldn't reorder anything.
            if launched {
                if let Some(g) = st.games.get_mut(i) {
                    g.last_played = Some(now_unix());
                }
                resort_recency(&mut st);
                last_used.clear();
            }
        }
        if st.sound_select {
            st.sound_select = false;
            audio.select();
        }
        if st.sound_tab {
            st.sound_tab = false;
            audio.tab();
        }
        // Settings changed in the Settings tab — apply live + persist.
        let settings_now = (
            st.audio_enabled,
            st.audio_volume,
            st.summon_tilt,
            st.panel_dist,
            st.panel_scale,
            st.panel_curve,
            st.playspace_x,
            st.playspace_y,
            st.playspace_z,
            st.playspace_yaw,
            st.uevr_delay,
        );
        if settings_now != settings_prev {
            audio.set_enabled(st.audio_enabled);
            audio.set_volume(st.audio_volume);
            settings_prev = settings_now;
            monadeck_core::overlay_config::OverlayConfig {
                audio_enabled: st.audio_enabled,
                audio_volume: st.audio_volume,
                summon_tilt: st.summon_tilt,
                panel_dist: st.panel_dist,
                panel_scale: st.panel_scale,
                panel_curve: st.panel_curve,
                playspace_x: st.playspace_x,
                playspace_y: st.playspace_y,
                playspace_z: st.playspace_z,
                playspace_yaw: st.playspace_yaw,
                uevr_delay: st.uevr_delay,
                freeze_delay_secs: st.freeze_delay_secs,
            }
            .save();
        }
        // Per-game playspace edits (from the Playspace tab) -> persist. The
        // effective offset is pushed to libmonado at the top of the loop (which
        // also runs while hidden), so global edits land there too.
        if st.ps_game_save_request {
            st.ps_game_save_request = false;
            if let Some(cid) = &running_cover {
                ps_overrides.insert(
                    cid.clone(),
                    [st.ps_game_x, st.ps_game_y, st.ps_game_z, st.ps_game_yaw],
                );
                monadeck_core::playspace_overrides::save(&ps_overrides);
                st.ps_game_override = true;
            }
        }
        if st.ps_game_clear_request {
            st.ps_game_clear_request = false;
            if let Some(cid) = &running_cover {
                ps_overrides.remove(cid);
                monadeck_core::playspace_overrides::save(&ps_overrides);
            }
            st.ps_game_override = false;
        }
        // Changing the distance re-places the dashboard in front of you.
        if st.panel_dist != panel_dist_prev {
            panel_dist_prev = st.panel_dist;
            recenter = true;
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
        if let Some(i) = st.uevr_toggle_request.take() {
            if let Some(g) = st.games.get_mut(i) {
                g.uevr = !g.uevr;
                if let Some(id) = &g.cover_id {
                    if g.uevr {
                        uevr_games.insert(id.clone());
                    } else {
                        uevr_games.remove(id);
                    }
                }
                monadeck_core::uevr::save_enabled(&uevr_games);
            }
        }
        // Collections: create / toggle the selected game's membership / delete.
        let mut cols_dirty = false;
        if let Some(name) = st.collection_create.take() {
            collections.push(monadeck_core::collections::Collection { name, members: Vec::new() });
            cols_dirty = true;
        }
        if let Some(ci) = st.collection_toggle.take() {
            if let Some(id) = st.selected.and_then(|i| st.games.get(i)).and_then(|g| g.cover_id.clone()) {
                monadeck_core::collections::toggle_member(&mut collections, ci, &id);
                cols_dirty = true;
            }
        }
        if let Some(ci) = st.collection_delete.take() {
            if ci < collections.len() {
                collections.remove(ci);
                cols_dirty = true;
            }
        }
        if cols_dirty {
            monadeck_core::collections::save(&collections);
            st.collections = collections.iter().map(|c| c.name.clone()).collect();
            apply_user_meta(&mut st.games, &favorites, &uevr_games, &playtime, &collections);
        }
        if st.refresh_request {
            st.refresh_request = false;
            if refresh_rx.is_none() {
                refresh_rx = Some(games::spawn_scan());
                manual_refresh = true;
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
        if let Some(id) = st.freeze_toggle_request.take() {
            let frozen = st.monado_clients.iter().find(|c| c.id == id).map(|c| c.frozen).unwrap_or(false);
            let counting_down = matches!(pending_freeze, Some((p, _)) if p == id);
            if frozen {
                // Unfreeze is immediate.
                monado.set_freeze(id, false);
                pending_freeze = None;
            } else if counting_down {
                // Tapping again during the countdown cancels it.
                pending_freeze = None;
            } else if st.freeze_delay_secs > 0.0 {
                // Count down first so the user can settle into position.
                pending_freeze = Some((id, now + std::time::Duration::from_secs_f32(st.freeze_delay_secs)));
            } else {
                monado.set_freeze(id, true);
            }
        }
        // Fire a pending freeze when its countdown elapses; expose the remaining
        // seconds for the button label meanwhile.
        match pending_freeze {
            Some((id, deadline)) if now >= deadline => {
                monado.set_freeze(id, true);
                pending_freeze = None;
                st.freeze_pending = None;
            }
            Some((id, deadline)) => st.freeze_pending = Some((id, (deadline - now).as_secs_f32())),
            None => st.freeze_pending = None,
        }
        if let Some(id) = st.set_active_request.take() {
            monado.set_primary(id);
        }
        if let Some(name) = st.kill_request.take() {
            stop_game(&name);
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

/// Launch a game through UEVR ("VR Mod") via the chihuahua injector under Proton.
/// Works for non-Steam shortcuts (exe + working dir from `shortcuts.vdf`) and
/// Proton Steam games (install dir probed for the shipping binary). Falls back to
/// a plain launch if the data is missing or the injector can't be spawned.
/// chihuahua is located by core (downloaded on first run; see `ensure_chihuahua`).
fn launch_uevr(g: &games::LibGame, delay: u32) {
    // protontricks `--appid`: the Steam appid for Steam games, or the non-Steam
    // shortcut's unsigned appid (which names its compatdata prefix) otherwise.
    let appid = g.shortcut_id.as_deref().or(g.app_id.as_deref());
    match (appid, g.start_dir.as_deref()) {
        (Some(appid), Some(start_dir)) => {
            // Steam games have no explicit exe here — the shipping binary is found
            // by probing `start_dir`; non-Steam shortcuts pass their launch exe.
            let exe = g.exe.as_deref().unwrap_or("");
            let opts = monadeck_core::uevr::LaunchOpts { delay, ..Default::default() };
            if let Err(e) = monadeck_core::uevr::launch(appid, exe, start_dir, &opts) {
                log::warn!("UEVR launch failed for '{}': {e} — falling back to a normal launch", g.name);
                launch_game(g);
            }
        }
        _ => {
            log::warn!("'{}' is flagged for UEVR but has no injectable launch path; launching normally", g.name);
            launch_game(g);
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
