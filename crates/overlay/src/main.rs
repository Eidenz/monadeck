// monadeck-overlay — in-headset VR game-library launcher for Monado.
//
// An OpenXR overlay session (XR_EXTX_overlay) that renders one large
// SteamVR-dashboard-style egui panel as a composition-layer quad, with a
// controller laser pointer for selection and grip-to-move. Game discovery and
// cover art come from monadeck-core (shared with the desktop launcher).
//
// The OpenXR/Vulkan/egui/laser plumbing is adapted from monado-frame.

mod audio;
mod desktop;
mod games;
mod gfx;
mod mathx;
mod monado;
mod notifications;
mod photos;
mod shots;
mod sky;
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
    /// Optional app icon (uploaded to the toast panel on first draw).
    icon: Option<egui::ColorImage>,
    icon_tex: Option<egui::TextureHandle>,
}

/// The SteamVR-style game-launch popup: its own composition layer (so it
/// survives the dashboard closing), placed in front of the head when the game is
/// launched. Lives until the game is detected running, or `deadline` passes. The
/// hero art is decoded on a background thread into `hero` (loaded into the popup
/// panel's own egui context — textures are per-context).
struct LaunchPopup {
    name: String,
    /// VR-Mod (UEVR) launch — its status switches to the injection-wait message.
    uevr: bool,
    pose: xr::Posef,
    started: Instant,
    deadline: Instant,
    hero: games::ArtState,
    /// Pending background decode of the game's hero art (None once resolved).
    hero_rx: Option<std::sync::mpsc::Receiver<Option<egui::ColorImage>>>,
}

impl LaunchPopup {
    /// The status line under the title. A UEVR launch shows a brief "Starting…"
    /// then the injection-wait message (chihuahua waits before injecting).
    fn status(&self) -> &'static str {
        if self.uevr && self.started.elapsed().as_secs_f32() > 1.5 {
            "Waiting for VR Mod injection…"
        } else {
            "Starting…"
        }
    }
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
        icon: None,
        icon_tex: None,
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
    // `monadeck-overlay --desktop-selftest`: exercise the desktop-viewer capture
    // path (portal → PipeWire → Vulkan import → PNG) with no headset/runtime.
    if std::env::args().any(|a| a == "--notify-selftest") {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
        let n = notifications::Notifications::start(true, true);
        println!("dbus={} udp={} — listening 12 s; send `notify-send` / an XSO UDP packet", n.dbus_ok, n.udp_ok);
        let t = Instant::now();
        while t.elapsed().as_secs() < 12 {
            for i in n.drain() {
                println!("  [{:?}] app={:?} title={:?} body={:?} timeout={} icon={}", i.source, i.app, i.title, i.body, i.timeout, i.icon.is_some());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        return;
    }
    if std::env::args().any(|a| a == "--keyboard-selftest") {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
        if let Err(e) = desktop::selftest::keyboard() {
            eprintln!("keyboard selftest FAILED: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    if std::env::args().any(|a| a == "--desktop-selftest") {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
        if let Err(e) = desktop::selftest::run() {
            eprintln!("desktop selftest FAILED: {e:#}");
            std::process::exit(1);
        }
        return;
    }
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
    // Per-layer opacity for mirrored desktop screens.
    let color_scale = available.khr_composition_layer_color_scale_bias;
    exts.khr_composition_layer_color_scale_bias = color_scale;
    // 360° background while no game runs.
    let equirect = available.khr_composition_layer_equirect2;
    exts.khr_composition_layer_equirect2 = equirect;
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
    // Desktop viewer: DMA-BUF import extensions (all-or-nothing; SHM fallback otherwise).
    let (dmabuf_exts, dmabuf_ok) = desktop::dmabuf::available_extensions(&vk_instance, physical_device);
    let device_create_info =
        vk::DeviceCreateInfo::default().queue_create_infos(&queue_infos).enabled_extension_names(&dmabuf_exts);
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
    // STAGE (floor, tracking origin) is stable across sessions; LOCAL is
    // re-anchored at the head each start. Desktop layouts are stored in STAGE.
    let stage_space = session.create_reference_space(xr::ReferenceSpaceType::STAGE, xr::Posef::IDENTITY).ok();
    if stage_space.is_none() {
        log::warn!("no STAGE reference space; desktop layouts will use LOCAL");
    }
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
    // The game-launch popup ("now starting" card). Its own layer, so it persists
    // after the dashboard auto-closes on launch (SteamVR-style); shows hero art.
    const LAUNCH_W: f32 = 0.58;
    let mut launch_panel = make_panel(
        &session, &device, allocator.clone(), render_pass, format, srgb,
        (840, 480), (LAUNCH_W, LAUNCH_W * 480.0 / 840.0), anchor,
    )?;
    let mut laser = make_laser(&session, format)?;
    // Wrist watch on the left controller (WayVR's offsets from the aim/tip pose).
    // ~WayVR's size (their watch is 0.115 m wide).
    const WATCH_W: f32 = 0.105;
    const WATCH_PX: (u32, u32) = (600, 404);
    let mut watch_panel = make_panel(
        &session, &device, allocator.clone(), render_pass, format, srgb,
        WATCH_PX, (WATCH_W, WATCH_W * WATCH_PX.1 as f32 / WATCH_PX.0 as f32), anchor,
    )?;
    // Default wrist spot = the user's tuned position (relative to the left aim pose).
    let watch_default = xr::Posef {
        position: xr::Vector3f { x: -0.041_881_38, y: -0.054_545_76, z: 0.110_306_92 },
        orientation: xr::Quaternionf { x: -0.703_661_44, y: -0.053_388_834, z: 0.691_710_65, w: -0.153_453_71 },
    };
    // (right-hand grab offset, last gripped pose) while the watch is being repositioned.
    let mut watch_grab: Option<(xr::Posef, xr::Posef)> = None;
    // Screenshots (monado-frame, folded in): watcher, wrist queue, photo windows, gallery.
    let mut ov_cfg = monadeck_core::overlay_config::OverlayConfig::load();
    if !ov_cfg.photos_settings_imported {
        // One-time import of monado-frame's settings.
        let mf = format!("{}/.config/monado-frame/config.json", std::env::var("HOME").unwrap_or_default());
        if let Ok(txt) = std::fs::read_to_string(&mf) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                ov_cfg.qr_detect = v["qr_detect"].as_bool().unwrap_or(ov_cfg.qr_detect);
                ov_cfg.qr_autodelete = v["qr_autodelete"].as_bool().unwrap_or(ov_cfg.qr_autodelete);
                ov_cfg.skip_wrist_photo = v["skip_wrist_photo"].as_bool().unwrap_or(ov_cfg.skip_wrist_photo);
                ov_cfg.skip_wrist_qr = v["skip_wrist_qr"].as_bool().unwrap_or(ov_cfg.skip_wrist_qr);
                ov_cfg.cleanup_days = v["cleanup_days"].as_i64().unwrap_or(ov_cfg.cleanup_days as i64) as i32;
                ov_cfg.crop_margin = v["crop_margin"].as_i64().unwrap_or(ov_cfg.crop_margin as i64) as i32;
                log::info!("photos: imported monado-frame settings from {mf}");
            }
        }
        ov_cfg.photos_settings_imported = true;
        ov_cfg.save();
    }
    let photo_cfg_of = |c: &monadeck_core::overlay_config::OverlayConfig| photos::PhotoCfg {
        qr_detect: c.qr_detect,
        qr_autodelete: c.qr_autodelete,
        skip_wrist_photo: c.skip_wrist_photo,
        skip_wrist_qr: c.skip_wrist_qr,
        cleanup_days: c.cleanup_days,
        crop_margin: c.crop_margin,
    };
    let mut photo_cfg = photo_cfg_of(&ov_cfg);
    let mut photos = photos::Photos::new(&session, &device, allocator.clone(), render_pass, format, srgb, &photo_cfg)?;
    let mut gestures = shots::gestures::load();
    let mut gestures_prev = gestures.clone();
    // Desktop + XSOverlay notifications → toasts (queued, one at a time).
    let notifications = notifications::Notifications::start(ov_cfg.notifications_enabled, ov_cfg.notifications_xso);
    let mut toast_queue: std::collections::VecDeque<(ToastState, f32)> = std::collections::VecDeque::new();

    // --- Actions ------------------------------------------------------------
    let action_set = xr_instance.create_action_set("monadeck", "monadeck overlay controls", 0)?;
    let left_path = xr_instance.string_to_path("/user/hand/left")?;
    let right_path = xr_instance.string_to_path("/user/hand/right")?;
    let aim_action = action_set.create_action::<xr::Posef>("aim", "Aim pose", &[left_path, right_path])?;
    let select_action = action_set.create_action::<f32>("select", "Select", &[left_path, right_path])?;
    let grab_action = action_set.create_action::<f32>("grab", "Grab", &[left_path, right_path])?;
    let scroll_action = action_set.create_action::<xr::Vector2f>("scroll", "Scroll", &[left_path, right_path])?;
    let system_action = action_set.create_action::<bool>("recenter", "Recenter panel", &[left_path, right_path])?;
    // Desktop viewer: A = right-click on a mirrored screen.
    let secondary_action = action_set.create_action::<bool>("secondary", "Secondary click", &[left_path, right_path])?;
    // B = a click that never drags (the cursor is frozen while held).
    let precise_action = action_set.create_action::<bool>("precise", "Precise click", &[left_path, right_path])?;
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
            xr::Binding::new(&secondary_action, xr_instance.string_to_path("/user/hand/left/input/a/click")?),
            xr::Binding::new(&secondary_action, xr_instance.string_to_path("/user/hand/right/input/a/click")?),
            xr::Binding::new(&precise_action, xr_instance.string_to_path("/user/hand/left/input/b/click")?),
            xr::Binding::new(&precise_action, xr_instance.string_to_path("/user/hand/right/input/b/click")?),
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
    // Desktop viewer (WayVR-style screen mirror): GPU caps + portal token.
    let desktop_caps = desktop::dmabuf::Caps::query(&vk_instance, physical_device, dmabuf_ok, format);
    let desktop_importer = desktop_caps
        .dmabuf
        .then(|| desktop::dmabuf::Importer::new(&vk_instance, &device, queue_family_index, format));
    let mut desktop = desktop::DesktopViewer::new(
        desktop_caps,
        desktop_importer,
        ov_cfg.screencast_token.clone(),
        ov_cfg.screen_order.clone(),
    );
    // The VR keyboard's own panel (egui) — drawn on demand, docks under screens.
    let kb_px = desktop::keyboard::panel_px();
    let mut kb_panel = make_panel(
        &session, &device, allocator.clone(), render_pass, format, srgb,
        kb_px, desktop::keyboard::size_m(), anchor,
    )?;
    desktop.set_width(ov_cfg.screen_width_m);
    desktop.caps.curved = curved;
    desktop.caps.color_scale = color_scale;
    desktop.gaze_pause = ov_cfg.gaze_pause;
    desktop.set_capture_limits(ov_cfg.capture_max_fps, ov_cfg.capture_max_height);
    let mut watch_offset = ov_cfg.watch_offset.map(arr_to_pose).unwrap_or(watch_default);
    // Watch time zones (bad names are skipped with a warning).
    let watch_zones: Vec<(String, chrono_tz::Tz)> = ov_cfg
        .watch_timezones
        .iter()
        .filter_map(|n| match n.parse::<chrono_tz::Tz>() {
            Ok(tz) => Some((n.rsplit('/').next().unwrap_or(n).replace('_', " "), tz)),
            Err(_) => {
                log::warn!("watch: unknown time zone '{n}'");
                None
            }
        })
        .collect();
    desktop.keyboard.scale = ov_cfg.keyboard_scale.clamp(0.5, 2.0);
    log::info!("desktop: curved={curved} opacity={color_scale}");
    let mut screencast_token = ov_cfg.screencast_token.clone();
    let mut sky = if equirect {
        Some(sky::Sky::load(ov_cfg.skybox_path.clone()))
    } else {
        log::warn!("runtime lacks XR_KHR_composition_layer_equirect2; no 360° background");
        None
    };
    // Named screen arrangements; the last used one is re-applied when the
    // screens come up (if enabled) so nothing has to be re-placed by hand.
    let mut layouts = monadeck_core::desktop_layouts::load();
    // With a saved approval the portal answers silently — ask right away so the
    // bar fills in without a click (and layouts can restore).
    if screencast_token.is_some() {
        desktop.setup_screens();
    }
    if ov_cfg.restore_layout {
        if let Some(l) = layouts.last_used.clone().and_then(|n| layouts.find(&n).cloned()) {
            log::info!("desktop: will restore layout '{}'", l.name);
            desktop.apply(&l);
        }
    }
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
        (ov_cfg.screen_width_m, ov_cfg.restore_layout, ov_cfg.watch_enabled, ov_cfg.gaze_pause, ov_cfg.keyboard_scale, ov_cfg.watch_24h, ov_cfg.watch_locked, ov_cfg.recenter_on_toggle, (ov_cfg.capture_max_fps, ov_cfg.capture_max_height, ov_cfg.skybox_enabled, ov_cfg.notifications_enabled, ov_cfg.notifications_xso, ov_cfg.notifications_sound)),
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
    st.screen_width_m = ov_cfg.screen_width_m;
    st.restore_layout = ov_cfg.restore_layout;
    st.gaze_pause = ov_cfg.gaze_pause;
    st.recenter_on_toggle = ov_cfg.recenter_on_toggle;
    st.capture_max_fps = ov_cfg.capture_max_fps;
    st.capture_max_height = ov_cfg.capture_max_height;
    st.skybox_enabled = ov_cfg.skybox_enabled;
    st.skybox_source = sky.as_ref().map(|s| s.source.clone()).unwrap_or_else(|| "unsupported by runtime".into());
    st.gesture_enabled = gestures.enabled;
    st.gesture_hold_ms = gestures.hold_ms as f32;
    st.gesture_feedback = gestures.frame_feedback;
    st.photo_qr_detect = ov_cfg.qr_detect;
    st.photo_qr_autodelete = ov_cfg.qr_autodelete;
    st.photo_skip_wrist = ov_cfg.skip_wrist_photo;
    st.photo_skip_wrist_qr = ov_cfg.skip_wrist_qr;
    st.photo_cleanup_days = ov_cfg.cleanup_days as f32;
    st.photo_crop_margin = ov_cfg.crop_margin as f32;
    st.photo_translate_ok = photos.translate_ok;
    st.photo_share_ok = photos.share_ok;
    st.photo_dir = photos.dir.clone();
    st.notif_enabled = ov_cfg.notifications_enabled;
    st.notif_xso = ov_cfg.notifications_xso;
    st.notif_sound = ov_cfg.notifications_sound;
    st.notif_dbus_ok = notifications.dbus_ok;
    st.notif_udp_ok = notifications.udp_ok;
    let mut nav_prev = st.nav;
    st.watch_enabled = ov_cfg.watch_enabled;
    st.watch_24h = ov_cfg.watch_24h;
    st.watch_locked = ov_cfg.watch_locked;
    st.keyboard_scale = ov_cfg.keyboard_scale;
    st.layout_active = layouts.last_used.clone();
    st.layouts = layouts.layouts.iter().map(|l| (l.name.clone(), l.screens.iter().filter(|s| s.shown).count())).collect();
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
    // The active launch popup (own layer; persists after the dashboard closes).
    let mut launch_popup: Option<LaunchPopup> = None;
    // A freeze counting down before it applies: (client id, deadline).
    let mut pending_freeze: Option<(u32, Instant)> = None;
    let mut click_prev = false; // haptic click edge
    // Laser fade over mirrored screens: (screen index, when the ray entered it).
    let mut screen_laser_since: Option<(usize, Instant)> = None;
    // Double-tap B on the LEFT controller toggles all screens (+ keyboard).
    let mut left_b_prev = false;
    let mut left_b_last: Option<Instant> = None;
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
                }
            }
            sys_prev = sys_down;
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

        // Incoming notifications → the toast queue (respecting the toggles).
        for n in notifications.drain() {
            let allowed = match n.source {
                notifications::Source::Desktop => st.notif_enabled,
                notifications::Source::XsOverlay => st.notif_xso,
            };
            if !allowed {
                continue;
            }
            let title = if n.app.is_empty() || n.app == n.title { n.title.clone() } else { format!("{} · {}", n.app, n.title) };
            toast_queue.push_back((
                ToastState { title, body: n.body, kind: ui::ToastKind::Notification, pose: xr::Posef::IDENTITY, until: now, icon: n.icon, icon_tex: None },
                n.timeout,
            ));
        }
        if st.notif_test_request {
            st.notif_test_request = false;
            toast_queue.push_back((
                ToastState { title: "Monadeck · Test".into(), body: "This is how a desktop or XSOverlay notification looks.".into(), kind: ui::ToastKind::Notification, pose: xr::Posef::IDENTITY, until: now, icon: None, icon_tex: None },
                5.0,
            ));
        }
        // Expire + render the toast on its own layer (shows even over a game).
        if toast.as_ref().map_or(false, |t| now >= t.until) {
            toast = None;
        }
        if toast.is_none() {
            if let Some((mut t, secs)) = toast_queue.pop_front() {
                if let Some(h) = hmd {
                    t.pose = mathx::toast_pose(&h, 1.3, 0.42);
                    t.until = now + std::time::Duration::from_secs_f32(secs);
                    if st.notif_sound {
                        audio.tab();
                    }
                    toast = Some(t);
                } else {
                    toast_queue.push_front((t, secs));
                }
            }
        }
        let toast_active = toast.is_some();
        if let Some(t) = &mut toast {
            if let Some(img) = t.icon.take() {
                t.icon_tex = Some(toast_panel.ctx.load_texture("toast-icon", img, egui::TextureOptions::LINEAR));
            }
            toast_panel.pose = t.pose;
            let (title, body, kind, icon_tex) = (t.title.clone(), t.body.clone(), t.kind, t.icon_tex.clone());
            render_panel(
                &mut toast_panel, &device, render_pass, cmd, cmd_pool, queue, fence,
                true, None, (0.0, 0.0), start.elapsed().as_secs_f64(),
                |ctx| ui::build_toast(ctx, &title, &body, kind, icon_tex.as_ref()),
            )?;
        }

        // --- Launch popup (own layer; runs even while the dashboard is hidden) ---
        // Pump the background hero decode, then dismiss the popup when the game
        // shows up as a running client (it's up — its own frames take over) or the
        // deadline passes (gave up waiting / it never started). Render it onto its
        // dedicated panel so the quad below can be composited in either path.
        if let Some(p) = &mut launch_popup {
            if let Some(rx) = &p.hero_rx {
                if let Ok(img) = rx.try_recv() {
                    p.hero = match img {
                        Some(img) => games::ArtState::Ready(launch_panel.ctx.load_texture(
                            "launch-hero",
                            img,
                            egui::TextureOptions::LINEAR,
                        )),
                        None => games::ArtState::Missing,
                    };
                    p.hero_rx = None;
                }
            }
            let appeared = running.as_ref().map_or(false, |app| name_matches(&p.name, app));
            if appeared || now >= p.deadline {
                launch_popup = None;
            }
        }
        let popup_active = launch_popup.is_some();
        if let Some(p) = &launch_popup {
            launch_panel.pose = p.pose;
            let (name, status, hero) = (&p.name, p.status(), &p.hero);
            render_panel(
                &mut launch_panel, &device, render_pass, cmd, cmd_pool, queue, fence,
                true, None, (0.0, 0.0), start.elapsed().as_secs_f64(),
                |ctx| ui::build_launch_popup(ctx, name, status, hero),
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

        // --- Controller freeze: countdown + apply (runs even while hidden) --
        // Handled before the visibility gate so a freeze armed from the Monado
        // page still fires after you close the overlay — otherwise the countdown
        // was stuck until you reopened it. The toggle request itself can only be
        // set while the dashboard is up, but the delayed apply must not be.
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

        // --- Desktop viewer (runs hidden or not): controller state, portal,
        // frame uploads, first placement. Screens persist while dismissed.
        let mut hands: Vec<desktop::HandInput> = Vec::new();
        if focused {
            for (aim, path) in [(&aim_left, left_path), (&aim_right, right_path)] {
                let located = locate_pose(aim, &space, time);
                let s = scroll_action.state(&session, path)?.current_state;
                hands.push(desktop::HandInput {
                    active: located.is_some(),
                    aim: located.unwrap_or(xr::Posef::IDENTITY),
                    path,
                    select: select_action.state(&session, path)?.current_state > 0.5,
                    secondary: secondary_action.state(&session, path)?.current_state,
                    precise: precise_action.state(&session, path)?.current_state,
                    grip: grab_action.state(&session, path)?.current_state,
                    scroll: deadzone(s.x, s.y),
                });
            }
        }
        desktop.set_local_in_stage(stage_space.as_ref().and_then(|st| locate_pose(&space, st, time)));
        desktop.poll(&session, &device, &allocator, cmd, queue, fence, hmd.as_ref());
        if let Some(tok) = desktop.take_token_change() {
            screencast_token = tok;
            overlay_config_from(&st, &screencast_token, &desktop.order(), &ov_cfg.watch_timezones, Some(pose_to_arr(&watch_offset)), &ov_cfg.skybox_path).save();
        }
        st.keyboard_shown = desktop.keyboard_visible();
        st.desktop_bar = desktop.bar_items();

        // --- Screenshots: watch the folder, feed the wrist card ----------------
        photos.poll(&photo_cfg, hmd.as_ref());
        if photos.notify_pulse {
            pulse(&session, &haptic_action, left_path, 0.4, 25);
            audio.tab();
        }
        photos.wrist_textures(&watch_panel.ctx);
        st.wrist_shot = photos.pending.get(photos.pending_idx).map(|p| ui::WristShot {
            thumb: p.thumb.clone(),
            qr: p.qr.clone(),
            when: p.when.clone(),
            idx: photos.pending_idx,
            total: photos.pending.len(),
        });
        // Gallery follows the Photos page.
        if visible && st.nav == ui::Nav::Photos {
            photos.gallery_open();
            photos.gallery_textures(&main_panel.ctx);
            st.gallery_items = photos.gallery.items.clone();
            st.gallery_page = photos.gallery.page;
            st.gallery_pages = photos.gallery_pages();
            st.gallery_total = photos.gallery_total();
            st.gallery_loading = photos.gallery.loading;
        } else if nav_prev == ui::Nav::Photos || !visible {
            if photos.gallery_active() {
                photos.gallery_close();
                st.gallery_items.clear();
            }
        }
        nav_prev = st.nav;
        let gr = std::mem::take(&mut st.gallery_req);
        if gr.open.is_some() || gr.delete.is_some() || gr.prev || gr.next || gr.refresh {
            photos.gallery_apply(gr, hmd.as_ref());
        }
        // Gesture + photo settings → persist on change.
        gestures.enabled = st.gesture_enabled;
        gestures.hold_ms = st.gesture_hold_ms.round() as i32;
        gestures.frame_feedback = st.gesture_feedback;
        if gestures != gestures_prev {
            shots::gestures::save(&gestures);
            gestures_prev = gestures.clone();
        }
        let pc = photos::PhotoCfg {
            qr_detect: st.photo_qr_detect,
            qr_autodelete: st.photo_qr_autodelete,
            skip_wrist_photo: st.photo_skip_wrist,
            skip_wrist_qr: st.photo_skip_wrist_qr,
            cleanup_days: st.photo_cleanup_days.round() as i32,
            crop_margin: st.photo_crop_margin.round() as i32,
        };
        if pc != photo_cfg {
            photo_cfg = pc;
            overlay_config_from(&st, &screencast_token, &desktop.order(), &ov_cfg.watch_timezones, Some(pose_to_arr(&watch_offset)), &ov_cfg.skybox_path).save();
        }

        // --- 360° background: upload once, show only while no game runs ------
        if let Some(s) = &mut sky {
            if let Err(e) = s.poll(&session, &device, &allocator, format, cmd, queue, fence) {
                log::error!("sky: {e}");
                sky = None;
            }
        }
        let show_sky = st.skybox_enabled && running.is_none();
        let sky_layer = if show_sky { sky.as_ref().and_then(|s| s.layer(&space)) } else { None };

        // --- Wrist watch (left controller; runs hidden or not) ------------------
        let tfmt = if st.watch_24h { "%H:%M" } else { "%-I:%M %p" };
        st.clock = chrono::Local::now().format(tfmt).to_string();
        st.watch_date = chrono::Local::now().format("%a %d/%m/%y").to_string();
        let utc = chrono::Utc::now();
        let zfmt = if st.watch_24h { "%H:%M" } else { "%-I:%M %p" };
        st.watch_times = watch_zones.iter().map(|(l, tz)| (l.clone(), utc.with_timezone(tz).format(zfmt).to_string())).collect();
        if st.watch_reset_request {
            st.watch_reset_request = false;
            watch_offset = watch_default;
            overlay_config_from(&st, &screencast_token, &desktop.order(), &ov_cfg.watch_timezones, Some(pose_to_arr(&watch_offset)), &ov_cfg.skybox_path).save();
        }
        st.watch_freeze_client = running.as_ref().and_then(|app| {
            st.monado_clients.iter().find(|c| name_matches(&c.name, app)).map(|c| (c.id, c.frozen))
        });
        let left_aim_pose = locate_pose(&aim_left, &space, time);
        let right_hand = hands.get(1).filter(|h| h.active);
        // Repositioning: while gripped by the right hand the watch follows it;
        // on release the new left-hand-relative offset is remembered.
        let mut watch_pose = if st.watch_enabled { left_aim_pose.map(|p| pose_compose(&p, &watch_offset)) } else { None };
        if let Some((off, last)) = watch_grab {
            match right_hand {
                Some(h) if h.grip >= GRAB_RELEASE => {
                    let wp = pose_compose(&h.aim, &off);
                    watch_pose = Some(wp);
                    watch_grab = Some((off, wp));
                }
                _ => {
                    // Released: remember where it ended up, relative to the left hand.
                    watch_grab = None;
                    if let Some(l) = left_aim_pose {
                        watch_offset = pose_compose(&pose_invert(&l), &last);
                        watch_pose = Some(last);
                        overlay_config_from(&st, &screencast_token, &desktop.order(), &ov_cfg.watch_timezones, Some(pose_to_arr(&watch_offset)), &ov_cfg.skybox_path).save();
                        log::info!("watch: position saved");
                    }
                }
            }
        }
        // The right hand points at the watch; it wins over everything behind it.
        let watch_hit = match (&watch_pose, hands.get(1)) {
            (Some(wp), Some(h)) if h.active => raycast(&h.aim, wp, watch_panel.size_m).map(|(u, v, t)| (u, v, t, h.select, h.aim)),
            _ => None,
        };
        if let (Some((_, _, _, _, _)), Some(h)) = (watch_hit, right_hand) {
            if !st.watch_locked && watch_grab.is_none() && h.grip > GRAB_START {
                if let Some(wp) = watch_pose {
                    watch_grab = Some((pose_compose(&pose_invert(&h.aim), &wp), wp));
                }
            }
        }
        let watch_hit = if watch_grab.is_some() { None } else { watch_hit };
        let watch_busy = watch_hit.is_some() || watch_grab.is_some();
        let watch_active = watch_pose.is_some();
        if let Some(wp) = watch_pose {
            watch_panel.pose = wp;
            let ptr = watch_hit.map(|(u, v, _, d, _)| (u, v, d));
            render_panel(
                &mut watch_panel, &device, render_pass, cmd, cmd_pool, queue, fence,
                true, ptr, (0.0, 0.0), start.elapsed().as_secs_f64(),
                |ctx| ui::build_watch(ctx, &mut st),
            )?;
        }
        let wr = std::mem::take(&mut st.wrist_req);
        if wr.open || wr.dismiss || wr.older || wr.newer {
            photos.wrist_apply(wr, hmd.as_ref());
        }
        if st.watch_menu_request {
            st.watch_menu_request = false;
            visible = !visible;
            if visible {
                recenter = true;
                summon_at = Some(Instant::now());
            }
        }
        // Double-B (left): hide every shown screen / bring the same set back.
        // Ignored while that hand is pointing at a screen (B = frozen click there).
        let left_b = hands.first().is_some_and(|h| h.active && h.precise);
        if left_b && !left_b_prev && desktop.pointing_hand() != Some(0) {
            let double = left_b_last.is_some_and(|t| t.elapsed().as_millis() < 450);
            if double {
                left_b_last = None;
                match desktop.toggle_all(hmd.as_ref(), st.recenter_on_toggle) {
                    desktop::ToggleAll::Hidden(n) => {
                        log::info!("desktop: double-B hid {n} item(s)");
                        audio.tab();
                    }
                    desktop::ToggleAll::Shown(n) => {
                        log::info!("desktop: double-B restored {n} item(s)");
                        audio.tab();
                    }
                    desktop::ToggleAll::Nothing => {
                        if let Some(h) = hmd {
                            let mut t = make_toast("No screen selected", "Show screens from the watch or the bottom bar", ui::ToastKind::Info, &h);
                            t.until = Instant::now() + std::time::Duration::from_millis(1500);
                            toast = Some(t);
                        }
                    }
                }
            } else {
                left_b_last = Some(Instant::now());
            }
        }
        left_b_prev = left_b;

        // Watch buttons must work with the dashboard dismissed, so these
        // requests drain here rather than in the visible-only path below.
        if st.keyboard_toggle_request {
            st.keyboard_toggle_request = false;
            desktop.toggle_keyboard();
        }
        if let Some(i) = st.desktop_bar_toggle.take() {
            desktop.toggle_bar(i);
        }
        if st.recenter_playspace_request {
            st.recenter_playspace_request = false;
            monado.recenter();
        }
        if st.sound_tab {
            st.sound_tab = false;
            audio.tab();
        }
        if st.sound_select {
            st.sound_select = false;
            audio.select();
        }
        // Desktop layouts: create / overwrite / apply / delete.
        let mut layouts_dirty = false;
        if let Some(name) = st.layout_create.take() {
            layouts.upsert(desktop.snapshot(name.clone()));
            layouts.last_used = Some(name);
            layouts_dirty = true;
        }
        if let Some(i) = st.layout_overwrite.take() {
            if let Some(name) = layouts.layouts.get(i).map(|l| l.name.clone()) {
                layouts.upsert(desktop.snapshot(name.clone()));
                layouts.last_used = Some(name);
                layouts_dirty = true;
            }
        }
        if let Some(i) = st.layout_apply.take() {
            if let Some(l) = layouts.layouts.get(i).cloned() {
                desktop.apply(&l);
                layouts.last_used = Some(l.name);
                layouts_dirty = true;
            }
        }
        if let Some(i) = st.layout_delete.take() {
            layouts.remove(i);
            layouts_dirty = true;
        }
        if let Some((i, name)) = st.layout_renamed.take() {
            layouts.rename(i, name);
            layouts_dirty = true;
        }
        if let Some((i, d)) = st.layout_move.take() {
            layouts_dirty |= layouts.move_by(i, d);
        }
        if st.layout_cycle_request {
            st.layout_cycle_request = false;
            if !layouts.layouts.is_empty() {
                let cur = layouts.last_used.as_ref().and_then(|n| layouts.layouts.iter().position(|l| &l.name == n));
                let next = cur.map_or(0, |c| (c + 1) % layouts.layouts.len());
                let l = layouts.layouts[next].clone();
                desktop.apply(&l);
                layouts.last_used = Some(l.name);
                monadeck_core::desktop_layouts::save(&layouts);
                st.layout_active = layouts.last_used.clone();
            }
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
            // Mirrored screens stay interactive while the dashboard is away.
            let p_in = photos.update_input(&hands, watch_busy.then_some(0.0));
            let d_in = desktop.update_input(&hands, if watch_busy { Some(0.0) } else { p_in.hit_t }, hmd.as_ref());
            photos.render(&device, render_pass, cmd, cmd_pool, queue, fence, start.elapsed().as_secs_f64(), &p_in.ptr)?;
            let d_ray = d_in.ray.or(p_in.ray).or(watch_hit.map(|(_, _, t, _, aim)| (aim, t)));
            if let Some(g) = d_in.gesture {
                toast = Some(ToastState { title: g.title, body: g.body, kind: ui::ToastKind::Info, pose: g.pose, until: Instant::now() + std::time::Duration::from_millis(700), icon: None, icon_tex: None });
            }
            let want_block = desktop.pointing() || p_in.ray.is_some();
            if want_block != blocked_prev {
                monado.set_block(want_block);
                blocked_prev = want_block;
            }
            let kb_q = render_keyboard(&mut desktop, &mut kb_panel, d_in.keyboard_ptr, &device, render_pass, cmd, cmd_pool, queue, fence, start.elapsed().as_secs_f64(), &space)?;
            if desktop.keyboard.clicked {
                audio.select();
            }
            let laser_alpha = screen_laser_alpha(desktop.pointing_screen(), &mut screen_laser_since);
            let laser_q = match (d_ray, hmd) {
                (Some((aim, t)), Some(h)) if laser_alpha > 0.0 => {
                    fill_laser(&mut laser, &device, cmd, queue, fence, laser_alpha)?;
                    Some(laser_quad(&laser, &space, &aim, t, &h))
                }
                _ => None,
            };
            let (screen_quads, screen_cyls) = desktop.screen_layers(&space);
            let watch_q = watch_active.then(|| quad_layer(&watch_panel, &space, true));
            let (toast_q, popup_q);
            let mut layers: Vec<&xr::CompositionLayerBase<xr::Vulkan>> = Vec::new();
            if let Some(s) = &sky_layer {
                layers.push(s);
            }
            for q in &screen_quads {
                layers.push(q);
            }
            for c in &screen_cyls {
                layers.push(c);
            }
            if let Some(q) = &kb_q {
                layers.push(q);
            }
            let photo_qs = photos.layers(&space);
            for q in &photo_qs {
                layers.push(q);
            }
            if popup_active {
                popup_q = quad_layer(&launch_panel, &space, true);
                layers.push(&popup_q);
            }
            if toast_active {
                toast_q = quad_layer(&toast_panel, &space, true);
                layers.push(&toast_q);
            }
            if let Some(q) = &watch_q {
                layers.push(q);
            }
            if let Some(q) = &laser_q {
                layers.push(q);
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
        let mut dash_zone = false;
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
                    // The whole dashboard band (panels + the gaps between them) is
                    // dashboard territory: nothing behind it gets the ray.
                    let zone_w = main_w + 2.0 * (gap_m + rail_w);
                    let zone_h = main_h + 2.0 * (gap_m + bottom_h);
                    let in_zone = if curved {
                        raycast_cylinder(&p, &main_l.pose, main_l.radius, (zone_w / main_l.radius).min(std::f32::consts::PI * 0.95), zone_h).is_some()
                    } else {
                        raycast(&p, &main_panel.pose, (zone_w, zone_h)).is_some()
                    };
                    dash_zone |= in_zone;
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

        // Mirrored screens: one closer than the dashboard takes the pointer. Not
        // while a dashboard grab is in progress (the grip would grab both).
        // The dashboard is always composited over the screens, so when the ray
        // hits it, it wins outright (max_t = 0 hides everything behind it).
        if watch_busy {
            best = None;
            scroll = (0.0, 0.0);
        }
        let block = best.is_some() || dash_zone || watch_busy;
        let p_in = if grab.is_some() { photos.update_input(&[], None) } else { photos.update_input(&hands, block.then_some(0.0)) };
        let d_in = if grab.is_some() {
            desktop.update_input(&[], None, hmd.as_ref())
        } else {
            desktop.update_input(&hands, if block { Some(0.0) } else { p_in.hit_t }, hmd.as_ref())
        };
        photos.render(&device, render_pass, cmd, cmd_pool, queue, fence, start.elapsed().as_secs_f64(), &p_in.ptr)?;
        let d_ray = d_in.ray.or(p_in.ray).or(watch_hit.map(|(_, _, t, _, aim)| (aim, t)));
        if let Some(g) = d_in.gesture {
            toast = Some(ToastState { title: g.title, body: g.body, kind: ui::ToastKind::Info, pose: g.pose, until: Instant::now() + std::time::Duration::from_millis(700), icon: None, icon_tex: None });
        }
        if d_ray.is_some() {
            best = None;
            scroll = (0.0, 0.0);
        }
        let kb_q = render_keyboard(&mut desktop, &mut kb_panel, d_in.keyboard_ptr, &device, render_pass, cmd, cmd_pool, queue, fence, start.elapsed().as_secs_f64(), &space)?;
        if desktop.keyboard.clicked {
            audio.select();
        }
        // Feed the Desktop page.
        st.desktop_rows = desktop.rows();
        st.desktop_status = desktop.status();
        st.desktop_hid_error = desktop.hid_error.clone();
        st.desktop_dmabuf = desktop.caps.dmabuf;
        st.desktop_shown = desktop.shown_count();
        st.desktop_ready = desktop.portal_ready();
        st.desktop_pending = desktop.portal_pending();
        st.keyboard_layout = desktop.keyboard.labels.layout_name();

        let main_ptr = best.filter(|h| h.panel == PanelId::Main).map(|h| (h.u, h.v, h.down));
        let rail_ptr = best.filter(|h| h.panel == PanelId::Rail).map(|h| (h.u, h.v, h.down));
        let bottom_ptr = best.filter(|h| h.panel == PanelId::Bottom).map(|h| (h.u, h.v, h.down));
        let laser_ray = best.map(|h| (h.aim, h.t)).or(d_ray);

        // Block the game's controller input while pointing at the dashboard.
        let want_block = best.is_some() || desktop.pointing() || p_in.ray.is_some();
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

        // Full laser on the dashboard; on a mirrored screen it fades out after entry.
        let laser_alpha = if best.is_some() {
            screen_laser_since = None;
            1.0
        } else {
            screen_laser_alpha(desktop.pointing_screen(), &mut screen_laser_since)
        };
        if laser_ray.is_some() && laser_alpha > 0.0 {
            fill_laser(&mut laser, &device, cmd, queue, fence, laser_alpha)?;
        }

        // All three panels as curved cylinder segments (rail + bottom alpha so
        // they float as rounded cards); quad fallback if no cylinder support.
        // Declared out here so each layer outlives the pointer vec.
        let (main_cyl, rail_cyl, bottom_cyl);
        let (main_quad, rail_quad, bottom_quad);
        let laser_q = match (laser_ray, hmd) {
            (Some((aim, t)), Some(h)) if laser_alpha > 0.0 => Some(laser_quad(&laser, &space, &aim, t, &h)),
            _ => None,
        };
        let (screen_quads, screen_cyls) = desktop.screen_layers(&space);
        let mut layers: Vec<&xr::CompositionLayerBase<xr::Vulkan>> = Vec::new();
        if let Some(s) = &sky_layer {
            layers.push(s);
        }
        // Screens first: they sit behind the dashboard in the composite.
        for q in &screen_quads {
            layers.push(q);
        }
        for c in &screen_cyls {
            layers.push(c);
        }
        if let Some(q) = &kb_q {
            layers.push(q);
        }
        let photo_qs = photos.layers(&space);
        for q in &photo_qs {
            layers.push(q);
        }
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
        let (toast_q, popup_q);
        if popup_active {
            popup_q = quad_layer(&launch_panel, &space, true);
            layers.push(&popup_q);
        }
        if toast_active {
            toast_q = quad_layer(&toast_panel, &space, true);
            layers.push(&toast_q);
        }
        let watch_q = watch_active.then(|| quad_layer(&watch_panel, &space, true));
        if let Some(q) = &watch_q {
            layers.push(q);
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
                    } else {
                        launch_game(g);
                    }
                    audio.launch();
                    // Hand off to the standalone launch popup (SteamVR-style): close
                    // the dashboard and put a "now starting" card in front of the
                    // head. Assigning `launch_popup` below replaces any popup already
                    // showing, so launching another game just takes over (no stacking).
                    // It lives until the game appears as a running client or a short
                    // timeout — since we can't detect a crash/close mid-load, the popup
                    // gives up quickly rather than lingering: 15 s for a normal game,
                    // uevr_delay + 15 s for a UEVR game (chihuahua waits before injecting).
                    let timeout = if g.uevr { st.uevr_delay as u64 + 15 } else { 15 };
                    let pose = match hmd {
                        Some(h) => front_pose(&h, 1.25, 0.0, 0.0, false),
                        None => posef([0.0, 0.0, -1.25]),
                    };
                    // Decode the hero art on a worker; loaded into the popup panel's
                    // own egui context when it lands (textures are per-context).
                    let hero_rx = g.cover_id.clone().map(|id| {
                        let (tx, rx) = std::sync::mpsc::channel();
                        std::thread::spawn(move || {
                            let _ = tx.send(games::decode(&id, games::ArtKind::Hero));
                        });
                        rx
                    });
                    launch_popup = Some(LaunchPopup {
                        name: g.name.clone(),
                        uevr: g.uevr,
                        pose,
                        started: Instant::now(),
                        deadline: Instant::now() + std::time::Duration::from_secs(timeout),
                        hero: games::ArtState::Idle,
                        hero_rx,
                    });
                    visible = false;
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
            (st.screen_width_m, st.restore_layout, st.watch_enabled, st.gaze_pause, st.keyboard_scale, st.watch_24h, st.watch_locked, st.recenter_on_toggle, (st.capture_max_fps, st.capture_max_height, st.skybox_enabled, st.notif_enabled, st.notif_xso, st.notif_sound)),
        );
        if settings_now != settings_prev {
            audio.set_enabled(st.audio_enabled);
            audio.set_volume(st.audio_volume);
            desktop.set_width(st.screen_width_m);
            desktop.gaze_pause = st.gaze_pause;
            desktop.set_capture_limits(st.capture_max_fps, st.capture_max_height);
            desktop.keyboard.scale = st.keyboard_scale.clamp(0.5, 2.0);
            settings_prev = settings_now;
            overlay_config_from(&st, &screencast_token, &desktop.order(), &ov_cfg.watch_timezones, Some(pose_to_arr(&watch_offset)), &ov_cfg.skybox_path).save();
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
        // Desktop page + bottom bar actions.
        if st.desktop_setup_request {
            st.desktop_setup_request = false;
            desktop.setup_screens();
        }
        if st.desktop_reselect_request {
            st.desktop_reselect_request = false;
            desktop.reselect();
        }
        if let Some((i, d)) = st.desktop_move_request.take() {
            if desktop.move_order(i, d) {
                overlay_config_from(&st, &screencast_token, &desktop.order(), &ov_cfg.watch_timezones, Some(pose_to_arr(&watch_offset)), &ov_cfg.skybox_path).save();
            }
        }
        if let Some((i, o)) = st.desktop_opacity_request.take() {
            desktop.set_screen_opacity(i, o);
        }

        if layouts_dirty {
            monadeck_core::desktop_layouts::save(&layouts);
            st.layout_active = layouts.last_used.clone();
            st.layouts = layouts.layouts.iter().map(|l| (l.name.clone(), l.screens.iter().filter(|s| s.shown).count())).collect();
        }
        if let Some(id) = st.set_active_request.take() {
            monado.set_primary(id);
        }
        if let Some(name) = st.kill_request.take() {
            stop_game(&name);
        }
    }
}

/// Laser opacity over a mirrored screen: full when the ray enters, gone after
/// `SCREEN_LASER_FADE` seconds, so it shows where you landed without covering
/// the desktop. Leaving (or switching screens) resets the fade.
const SCREEN_LASER_FADE: f32 = 2.0;
fn screen_laser_alpha(screen: Option<usize>, since: &mut Option<(usize, Instant)>) -> f32 {
    match screen {
        None => {
            *since = None;
            1.0
        }
        Some(s) => {
            let entered = match *since {
                Some((prev, t)) if prev == s => t,
                _ => {
                    let now = Instant::now();
                    *since = Some((s, now));
                    now
                }
            };
            (1.0 - entered.elapsed().as_secs_f32() / SCREEN_LASER_FADE).clamp(0.0, 1.0)
        }
    }
}

/// Render the VR keyboard onto its panel (when visible) and build its layer.
#[allow(clippy::too_many_arguments)]
fn render_keyboard<'a>(
    desktop: &mut desktop::DesktopViewer,
    panel: &'a mut gfx::PanelGfx,
    pointer: Option<(f32, f32, bool)>,
    device: &ash::Device,
    render_pass: vk::RenderPass,
    cmd: vk::CommandBuffer,
    cmd_pool: vk::CommandPool,
    queue: vk::Queue,
    fence: vk::Fence,
    elapsed: f64,
    space: &'a xr::Space,
) -> Result<Option<xr::CompositionLayerQuad<'a, xr::Vulkan>>> {
    if !desktop.keyboard.visible || !desktop.keyboard.placed {
        desktop.keyboard.clicked = false;
        return Ok(None);
    }
    panel.pose = desktop.keyboard.pose;
    panel.size_m = desktop::keyboard::size_m_scaled(desktop.keyboard.scale);
    let kb = &mut desktop.keyboard;
    render_panel(panel, device, render_pass, cmd, cmd_pool, queue, fence, true, pointer, (0.0, 0.0), elapsed, |ctx| {
        desktop::keyboard::build(ctx, kb)
    })?;
    desktop.flush_keyboard();
    if !desktop.keyboard.visible {
        return Ok(None);
    }
    Ok(Some(quad_layer(panel, space, true)))
}

/// The persisted overlay preferences, from live UI state.
fn pose_to_arr(p: &xr::Posef) -> [f32; 7] {
    [p.position.x, p.position.y, p.position.z, p.orientation.x, p.orientation.y, p.orientation.z, p.orientation.w]
}

fn arr_to_pose(a: [f32; 7]) -> xr::Posef {
    xr::Posef {
        position: xr::Vector3f { x: a[0], y: a[1], z: a[2] },
        orientation: xr::Quaternionf { x: a[3], y: a[4], z: a[5], w: a[6] },
    }
}

fn overlay_config_from(
    st: &ui::LibState,
    screencast_token: &Option<String>,
    screen_order: &[String],
    watch_timezones: &[String],
    watch_offset: Option<[f32; 7]>,
    skybox_path: &Option<String>,
) -> monadeck_core::overlay_config::OverlayConfig {
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
        screencast_token: screencast_token.clone(),
        screen_width_m: st.screen_width_m,
        screen_order: screen_order.to_vec(),
        restore_layout: st.restore_layout,
        watch_enabled: st.watch_enabled,
        watch_timezones: watch_timezones.to_vec(),
        watch_24h: st.watch_24h,
        watch_locked: st.watch_locked,
        watch_offset,
        gaze_pause: st.gaze_pause,
        recenter_on_toggle: st.recenter_on_toggle,
        keyboard_scale: st.keyboard_scale,
        capture_max_fps: st.capture_max_fps,
        capture_max_height: st.capture_max_height,
        skybox_enabled: st.skybox_enabled,
        skybox_path: skybox_path.clone(),
        qr_detect: st.photo_qr_detect,
        qr_autodelete: st.photo_qr_autodelete,
        skip_wrist_photo: st.photo_skip_wrist,
        skip_wrist_qr: st.photo_skip_wrist_qr,
        cleanup_days: st.photo_cleanup_days.round() as i32,
        crop_margin: st.photo_crop_margin.round() as i32,
        photos_settings_imported: true,
        notifications_enabled: st.notif_enabled,
        notifications_xso: st.notif_xso,
        notifications_sound: st.notif_sound,
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
