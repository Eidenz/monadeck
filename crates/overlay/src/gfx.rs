// Vulkan + egui rendering plumbing for the overlay's composition-layer panels.
// Ported (and trimmed to a single panel + laser) from monado-frame.
use std::sync::{Arc, Mutex};

use anyhow::Result;
use ash::vk;
use ash::vk::Handle as _;
use openxr as xr;

use crate::mathx::{
    cross, forward, normalize, q_mul, qf, quat_from_axes, quat_from_axis_angle, quat_rotate, quatf,
    vec3f,
};

/// egui logical-pixel scale. Larger => crisper text at the cost of fill-rate.
pub const PPP: f32 = 1.5;

pub mod theme {
    use egui::Color32;
    // Monadeck teal accent over a deep SteamVR-like charcoal.
    pub const PRIMARY: Color32 = Color32::from_rgb(64, 224, 208);
    pub const SURFACE: Color32 = Color32::from_rgb(22, 26, 31);
    pub const SURFACE_CONTAINER: Color32 = Color32::from_rgb(30, 35, 42);
    pub const SURFACE_CONTAINER_HIGH: Color32 = Color32::from_rgb(42, 49, 58);
    pub const ON_SURFACE: Color32 = Color32::from_rgb(228, 233, 240);
    pub const ON_SURFACE_VAR: Color32 = Color32::from_rgb(160, 172, 186);
}

/// Monadeck's own glyphs, the ones Phosphor lacks, drawn on its grid:
/// `assets/fonts/monadeck-icons.otf` (built by `build_icons.py` beside it).
pub mod glyph {
    pub const QUEST_LEFT: &str = "\u{F0000}";
    pub const QUEST_RIGHT: &str = "\u{F0001}";
    pub const INDEX_LEFT: &str = "\u{F0002}";
    pub const INDEX_RIGHT: &str = "\u{F0003}";
}

/// egui's fonts + the Phosphor icons + Monadeck's own + a system CJK font as
/// the last fallback, so Japanese (and Chinese / Korean) titles don't come
/// out as boxes. Every panel uses this.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    fonts.font_data.insert(
        "monadeck-icons".into(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../assets/fonts/monadeck-icons.otf"))),
    );
    fonts.families.entry(egui::FontFamily::Proportional).or_default().push("monadeck-icons".into());
    if let Some(cjk) = cjk_font() {
        fonts.font_data.insert("cjk".into(), cjk);
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push("cjk".into());
        }
    }
    ctx.set_fonts(fonts);
}

/// The system's CJK font, read once and shared by every panel. egui copies an
/// owned font into each context, so the bytes are made `'static` (one copy
/// for the whole overlay, ~4–32 MB depending on the font).
fn cjk_font() -> Option<std::sync::Arc<egui::FontData>> {
    use ab_glyph::{Font, VariableFont};
    static FONT: std::sync::OnceLock<Option<std::sync::Arc<egui::FontData>>> = std::sync::OnceLock::new();
    FONT.get_or_init(|| {
        // egui can't pick a weight inside a variable font, so one renders at
        // its default instance (thin, for Noto Sans CJK VF): take it only if
        // nothing static turns up.
        let mut variable: Option<(std::path::PathBuf, u32, Vec<u8>)> = None;
        let mut pick = None;
        for (path, index, chosen) in cjk_candidates() {
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let Ok(font) = ab_glyph::FontRef::try_from_slice_and_index(&bytes, index) else {
                log::warn!("fonts: {} (face {index}) doesn't parse, skipping", path.display());
                continue;
            };
            // fontconfig answers with a Latin font when there's no CJK one.
            if font.glyph_id('東').0 == 0 {
                continue;
            }
            if !chosen && !font.variations().is_empty() {
                variable.get_or_insert((path, index, bytes));
                continue;
            }
            pick = Some((path, index, bytes));
            break;
        }
        let Some((path, index, bytes)) = pick.or(variable) else {
            log::warn!("fonts: no CJK font found; Japanese/Chinese/Korean text shows as boxes (install Noto Sans CJK)");
            return None;
        };
        log::info!("fonts: CJK fallback {} (face {index})", path.display());
        let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        let mut data = egui::FontData::from_static(bytes);
        data.index = index;
        Some(std::sync::Arc::new(data))
    })
    .clone()
}

/// Where to look for a CJK font, best first: `MONADECK_CJK_FONT=path[:face]`
/// (taken as is), the static Noto Sans CJK most distros ship, what fontconfig
/// picks for Japanese, then Droid Sans Fallback.
fn cjk_candidates() -> Vec<(std::path::PathBuf, u32, bool)> {
    let mut out = Vec::new();
    if let Ok(v) = std::env::var("MONADECK_CJK_FONT") {
        let (path, face) = match v.rsplit_once(':') {
            Some((p, f)) if f.parse::<u32>().is_ok() => (p.to_string(), f.parse().unwrap_or(0)),
            _ => (v, 0),
        };
        out.push((path.into(), face, true));
    }
    for p in [
        "/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto/NotoSansCJK-Regular.ttc",
    ] {
        out.push((p.into(), 0, false));
    }
    // fontconfig's choice for Japanese. Its index packs a named instance into
    // the high bits; the face is the low 16.
    if let Ok(o) = std::process::Command::new("fc-match").args(["-f", "%{file}|%{index}", "sans-serif:lang=ja"]).output() {
        let s = String::from_utf8_lossy(&o.stdout);
        if let Some((file, idx)) = s.split_once('|') {
            out.push((file.into(), idx.trim().parse::<u32>().unwrap_or(0) & 0xFFFF, false));
        }
    }
    out.push(("/usr/share/fonts/google-droid-sans-fonts/DroidSansFallbackFull.ttf".into(), 0, false));
    out.push(("/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf".into(), 0, false));
    out
}

pub fn apply_style(ctx: &egui::Context) {
    use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};
    let mut style = (*ctx.style()).clone();
    let mut v = egui::Visuals::dark();
    v.panel_fill = theme::SURFACE;
    v.window_fill = theme::SURFACE_CONTAINER;
    v.faint_bg_color = theme::SURFACE_CONTAINER;
    v.extreme_bg_color = Color32::from_rgb(14, 17, 21);
    v.override_text_color = Some(theme::ON_SURFACE);
    v.selection.bg_fill = Color32::from_rgb(20, 90, 84);
    v.selection.stroke = Stroke::new(1.0, theme::PRIMARY);
    v.hyperlink_color = theme::PRIMARY;
    v.widgets.noninteractive.bg_fill = theme::SURFACE;
    v.widgets.inactive.bg_fill = theme::SURFACE_CONTAINER_HIGH;
    v.widgets.inactive.weak_bg_fill = theme::SURFACE_CONTAINER_HIGH;
    v.widgets.hovered.bg_fill = Color32::from_rgb(48, 70, 74);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(48, 70, 74);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.active.bg_fill = theme::PRIMARY;
    v.widgets.active.weak_bg_fill = theme::PRIMARY;
    v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::BLACK);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(10);
        w.bg_stroke = Stroke::NONE;
    }
    style.visuals = v;
    style.spacing.item_spacing = egui::vec2(10.0, 12.0);
    // Plain text is never selectable in VR: otherwise every label senses clicks
    // (text selection) and the watch's clock "clicks" with a sound and a glow.
    style.interaction.selectable_labels = false;
    style.spacing.button_padding = egui::vec2(14.0, 9.0);
    style.spacing.interact_size.y = 30.0;
    style.text_styles.insert(TextStyle::Heading, FontId::new(26.0, FontFamily::Proportional));
    style.text_styles.insert(TextStyle::Body, FontId::new(17.0, FontFamily::Proportional));
    style.text_styles.insert(TextStyle::Button, FontId::new(17.0, FontFamily::Proportional));
    style.text_styles.insert(TextStyle::Small, FontId::new(13.0, FontFamily::Proportional));
    ctx.set_style(style);
}

pub struct PanelGfx {
    pub swapchain: xr::Swapchain<xr::Vulkan>,
    pub framebuffers: Vec<vk::Framebuffer>,
    pub ctx: egui::Context,
    pub renderer: egui_ash_renderer::Renderer,
    pub px: (u32, u32),
    pub size_m: (f32, f32),
    pub pose: xr::Posef,
    prev_pos: Option<egui::Pos2>,
    prev_down: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn make_panel(
    session: &xr::Session<xr::Vulkan>,
    device: &ash::Device,
    allocator: Arc<Mutex<gpu_allocator::vulkan::Allocator>>,
    render_pass: vk::RenderPass,
    format: vk::Format,
    srgb: bool,
    px: (u32, u32),
    size_m: (f32, f32),
    pose: xr::Posef,
) -> Result<PanelGfx> {
    let swapchain = session.create_swapchain(&xr::SwapchainCreateInfo {
        create_flags: xr::SwapchainCreateFlags::EMPTY,
        usage_flags: xr::SwapchainUsageFlags::COLOR_ATTACHMENT,
        format: format.as_raw() as _,
        sample_count: 1,
        width: px.0,
        height: px.1,
        face_count: 1,
        array_size: 1,
        mip_count: 1,
    })?;
    let images: Vec<vk::Image> =
        swapchain.enumerate_images()?.into_iter().map(vk::Image::from_raw).collect();
    let framebuffers = make_framebuffers(device, render_pass, format, &images, px)?;

    let ctx = egui::Context::default();
    install_fonts(&ctx);
    apply_style(&ctx);
    ctx.set_pixels_per_point(PPP);
    ctx.options_mut(|o| {
        o.input_options.max_click_dist = 80.0;
        o.input_options.max_click_duration = 3.0;
    });

    let renderer = egui_ash_renderer::Renderer::with_gpu_allocator(
        allocator,
        device.clone(),
        render_pass,
        egui_ash_renderer::Options { srgb_framebuffer: srgb, ..Default::default() },
    )
    .map_err(|e| anyhow::anyhow!("egui renderer init: {e}"))?;

    Ok(PanelGfx {
        swapchain,
        framebuffers,
        ctx,
        renderer,
        px,
        size_m,
        pose,
        prev_pos: None,
        prev_down: false,
    })
}

fn make_framebuffers(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    format: vk::Format,
    images: &[vk::Image],
    px: (u32, u32),
) -> Result<Vec<vk::Framebuffer>> {
    let range = vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    };
    let mut fbs = Vec::with_capacity(images.len());
    for &img in images {
        let view = unsafe {
            device.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(img)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(format)
                    .subresource_range(range),
                None,
            )?
        };
        let atts = [view];
        let fb = unsafe {
            device.create_framebuffer(
                &vk::FramebufferCreateInfo::default()
                    .render_pass(render_pass)
                    .attachments(&atts)
                    .width(px.0)
                    .height(px.1)
                    .layers(1),
                None,
            )?
        };
        fbs.push(fb);
    }
    Ok(fbs)
}

/// Run egui for this panel and rasterise it to the next swapchain image.
/// `pointer` is the laser hit in (u, v, down) panel-space, if any.
#[allow(clippy::too_many_arguments)]
pub fn render_panel(
    p: &mut PanelGfx,
    device: &ash::Device,
    render_pass: vk::RenderPass,
    cmd: vk::CommandBuffer,
    cmd_pool: vk::CommandPool,
    queue: vk::Queue,
    fence: vk::Fence,
    alpha_mode: bool,
    pointer: Option<(f32, f32, bool)>,
    scroll: (f32, f32),
    time: f64,
    mut build: impl FnMut(&egui::Context),
) -> Result<()> {
    let pos = pointer.map(|(u, v, _)| egui::pos2(u * p.px.0 as f32 / PPP, v * p.px.1 as f32 / PPP));
    let down = pointer.is_some_and(|(_, _, d)| d);

    let mut events = Vec::new();
    if let Some(ps) = pos {
        events.push(egui::Event::PointerMoved(ps));
    } else if p.prev_pos.is_some() {
        events.push(egui::Event::PointerGone);
    }
    // Thumbstick scroll, routed to whatever ScrollArea is under the pointer.
    // Stick up/right => content scrolls toward earlier/left, like a real wheel.
    if pos.is_some() && (scroll.0 != 0.0 || scroll.1 != 0.0) {
        const SPEED: f32 = 14.0;
        events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(scroll.0 * SPEED, scroll.1 * SPEED),
            modifiers: egui::Modifiers::default(),
        });
    }
    if down != p.prev_down {
        if let Some(ps) = pos.or(p.prev_pos) {
            events.push(egui::Event::PointerButton {
                pos: ps,
                button: egui::PointerButton::Primary,
                pressed: down,
                modifiers: egui::Modifiers::default(),
            });
        }
    }
    p.prev_pos = pos;
    p.prev_down = down;

    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(p.px.0 as f32 / PPP, p.px.1 as f32 / PPP),
        )),
        // A real, monotonic time enables egui animations + double-click timing.
        time: Some(time),
        events,
        ..Default::default()
    };

    let out = p.ctx.run(raw, |ctx| {
        build(ctx);
        if let Some(ps) = pos {
            let painter =
                ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("cursor")));
            painter.circle_filled(ps, 5.0, theme::PRIMARY);
            painter.circle_stroke(ps, 5.0, egui::Stroke::new(1.5, egui::Color32::from_black_alpha(150)));
        }
    });

    let prims = p.ctx.tessellate(out.shapes, out.pixels_per_point);
    p.renderer
        .set_textures(queue, cmd_pool, &out.textures_delta.set)
        .map_err(|e| anyhow::anyhow!("set_textures: {e}"))?;

    let index = p.swapchain.acquire_image()?;
    p.swapchain.wait_image(xr::Duration::INFINITE)?;
    let clear = if alpha_mode { [0.0, 0.0, 0.0, 0.0] } else { [0.05, 0.06, 0.08, 1.0] };
    unsafe {
        device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
        device.begin_command_buffer(
            cmd,
            &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
        )?;
        let clears = [vk::ClearValue { color: vk::ClearColorValue { float32: clear } }];
        let rp = vk::RenderPassBeginInfo::default()
            .render_pass(render_pass)
            .framebuffer(p.framebuffers[index as usize])
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D { width: p.px.0, height: p.px.1 },
            })
            .clear_values(&clears);
        device.cmd_begin_render_pass(cmd, &rp, vk::SubpassContents::INLINE);
        p.renderer
            .cmd_draw(cmd, vk::Extent2D { width: p.px.0, height: p.px.1 }, out.pixels_per_point, &prims)
            .map_err(|e| anyhow::anyhow!("cmd_draw: {e}"))?;
        device.cmd_end_render_pass(cmd);
        device.end_command_buffer(cmd)?;
        let cmds = [cmd];
        let submit = vk::SubmitInfo::default().command_buffers(&cmds);
        device.queue_submit(queue, &[submit], fence)?;
        device.wait_for_fences(&[fence], true, u64::MAX)?;
        device.reset_fences(&[fence])?;
    }
    p.renderer
        .free_textures(&out.textures_delta.free)
        .map_err(|e| anyhow::anyhow!("free_textures: {e}"))?;
    p.swapchain.release_image()?;
    Ok(())
}

pub fn quad_layer<'a>(
    p: &'a PanelGfx,
    space: &'a xr::Space,
    alpha_mode: bool,
) -> xr::CompositionLayerQuad<'a, xr::Vulkan> {
    let sub = xr::SwapchainSubImage::new().swapchain(&p.swapchain).image_array_index(0).image_rect(
        xr::Rect2Di {
            offset: xr::Offset2Di { x: 0, y: 0 },
            extent: xr::Extent2Di { width: p.px.0 as i32, height: p.px.1 as i32 },
        },
    );
    let mut q = xr::CompositionLayerQuad::new()
        .space(space)
        .eye_visibility(xr::EyeVisibility::BOTH)
        .sub_image(sub)
        .pose(p.pose)
        .size(xr::Extent2Df { width: p.size_m.0, height: p.size_m.1 });
    if alpha_mode {
        q = q.layer_flags(xr::CompositionLayerFlags::BLEND_TEXTURE_SOURCE_ALPHA);
    }
    q
}

/// A panel's placement on the shared dashboard cylinder. Used by both the layer
/// build and the hit-test, so they always agree.
pub struct CylLayout {
    pub pose: xr::Posef,
    pub radius: f32,
    pub central_angle: f32,
    pub height: f32,
}

/// Place a sub-panel on the dashboard cylinder. The axis sits `base_radius` ahead
/// of `anchor` (at the viewer); `radius` is this panel's surface distance (a touch
/// less than base to float in front); `yaw` shifts it left/right around the axis;
/// `y_off` shifts it up/down along the axis. So rail/bottom curve like the main.
#[allow(clippy::too_many_arguments)]
pub fn cyl_layout(
    anchor: &xr::Posef,
    base_radius: f32,
    radius: f32,
    yaw: f32,
    y_off: f32,
    width_m: f32,
    height_m: f32,
) -> CylLayout {
    let q = qf(&anchor.orientation);
    let z = quat_rotate(q, [0.0, 0.0, 1.0]); // toward viewer
    let up = quat_rotate(q, [0.0, 1.0, 0.0]);
    let axis = [
        anchor.position.x + z[0] * base_radius,
        anchor.position.y + z[1] * base_radius,
        anchor.position.z + z[2] * base_radius,
    ];
    let pos = [axis[0] + up[0] * y_off, axis[1] + up[1] * y_off, axis[2] + up[2] * y_off];
    let orient = q_mul(quat_from_axis_angle(up, yaw), q);
    let central_angle = (width_m / radius).min(std::f32::consts::PI * 0.9);
    CylLayout {
        pose: xr::Posef { orientation: quatf(orient), position: vec3f(pos) },
        radius,
        central_angle,
        height: height_m,
    }
}

pub fn cylinder_layer<'a>(
    p: &'a PanelGfx,
    space: &'a xr::Space,
    layout: &CylLayout,
    alpha: bool,
) -> xr::CompositionLayerCylinderKHR<'a, xr::Vulkan> {
    let sub = xr::SwapchainSubImage::new().swapchain(&p.swapchain).image_array_index(0).image_rect(
        xr::Rect2Di {
            offset: xr::Offset2Di { x: 0, y: 0 },
            extent: xr::Extent2Di { width: p.px.0 as i32, height: p.px.1 as i32 },
        },
    );
    let mut c = xr::CompositionLayerCylinderKHR::new()
        .space(space)
        .eye_visibility(xr::EyeVisibility::BOTH)
        .sub_image(sub)
        .pose(layout.pose)
        .radius(layout.radius)
        .central_angle(layout.central_angle)
        .aspect_ratio(p.px.0 as f32 / p.px.1 as f32);
    if alpha {
        c = c.layer_flags(xr::CompositionLayerFlags::BLEND_TEXTURE_SOURCE_ALPHA);
    }
    c
}

// --- Laser pointer ----------------------------------------------------------

pub struct Laser {
    pub swapchain: xr::Swapchain<xr::Vulkan>,
    images: Vec<vk::Image>,
}

pub fn make_laser(session: &xr::Session<xr::Vulkan>, format: vk::Format) -> Result<Laser> {
    let swapchain = session.create_swapchain(&xr::SwapchainCreateInfo {
        create_flags: xr::SwapchainCreateFlags::EMPTY,
        usage_flags: xr::SwapchainUsageFlags::COLOR_ATTACHMENT | xr::SwapchainUsageFlags::TRANSFER_DST,
        format: format.as_raw() as _,
        sample_count: 1,
        width: 8,
        height: 8,
        face_count: 1,
        array_size: 1,
        mip_count: 1,
    })?;
    let images = swapchain.enumerate_images()?.into_iter().map(vk::Image::from_raw).collect();
    Ok(Laser { swapchain, images })
}

/// Fill the laser texture with the accent colour at `alpha` (premultiplied, so
/// the layer can be blended over what's behind it). Called per shown frame.
pub fn fill_laser(
    laser: &mut Laser,
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    queue: vk::Queue,
    fence: vk::Fence,
    alpha: f32,
) -> Result<()> {
    let index = laser.swapchain.acquire_image()? as usize;
    laser.swapchain.wait_image(xr::Duration::INFINITE)?;
    let image = laser.images[index];
    let range = vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    };
    unsafe {
        device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
        device.begin_command_buffer(
            cmd,
            &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
        )?;
        let to_dst = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(range);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_dst],
        );
        let a = alpha.clamp(0.0, 1.0);
        let color = vk::ClearColorValue { float32: [0.25 * a, 0.88 * a, 0.81 * a, a] };
        device.cmd_clear_color_image(cmd, image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &color, &[range]);
        let to_src = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(range);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_src],
        );
        device.end_command_buffer(cmd)?;
        let cmds = [cmd];
        device.queue_submit(queue, &[vk::SubmitInfo::default().command_buffers(&cmds)], fence)?;
        device.wait_for_fences(&[fence], true, u64::MAX)?;
        device.reset_fences(&[fence])?;
    }
    laser.swapchain.release_image()?;
    Ok(())
}

/// A thin vertical bar (the laser texture) at `pose`, `height` tall — used as
/// the docking-edge indicator.
pub fn bar_quad<'a>(laser: &'a Laser, space: &'a xr::Space, pose: xr::Posef, height: f32) -> xr::CompositionLayerQuad<'a, xr::Vulkan> {
    let sub = xr::SwapchainSubImage::new().swapchain(&laser.swapchain).image_array_index(0).image_rect(
        xr::Rect2Di {
            offset: xr::Offset2Di { x: 0, y: 0 },
            extent: xr::Extent2Di { width: 8, height: 8 },
        },
    );
    xr::CompositionLayerQuad::new()
        .space(space)
        .eye_visibility(xr::EyeVisibility::BOTH)
        .sub_image(sub)
        .pose(pose)
        .size(xr::Extent2Df { width: 0.012, height })
        .layer_flags(xr::CompositionLayerFlags::BLEND_TEXTURE_SOURCE_ALPHA)
}

/// A thin quad from the controller to the hit point, billboarded toward the HMD.
/// Alpha-blended so a faded laser (see the desktop screens) shows through.
pub fn laser_quad<'a>(
    laser: &'a Laser,
    space: &'a xr::Space,
    aim: &xr::Posef,
    dist: f32,
    hmd: &xr::Posef,
) -> xr::CompositionLayerQuad<'a, xr::Vulkan> {
    let o = [aim.position.x, aim.position.y, aim.position.z];
    let dir = normalize(forward(aim));
    let mid = [o[0] + dir[0] * dist * 0.5, o[1] + dir[1] * dist * 0.5, o[2] + dir[2] * dist * 0.5];
    let to_view = normalize([hmd.position.x - mid[0], hmd.position.y - mid[1], hmd.position.z - mid[2]]);
    let x = normalize(cross(dir, to_view));
    let z = cross(x, dir);
    let q = quat_from_axes(x, dir, z);
    let sub = xr::SwapchainSubImage::new().swapchain(&laser.swapchain).image_array_index(0).image_rect(
        xr::Rect2Di {
            offset: xr::Offset2Di { x: 0, y: 0 },
            extent: xr::Extent2Di { width: 8, height: 8 },
        },
    );
    xr::CompositionLayerQuad::new()
        .space(space)
        .eye_visibility(xr::EyeVisibility::BOTH)
        .sub_image(sub)
        .pose(xr::Posef { orientation: quatf(q), position: vec3f(mid) })
        .size(xr::Extent2Df { width: 0.003, height: dist })
        .layer_flags(xr::CompositionLayerFlags::BLEND_TEXTURE_SOURCE_ALPHA)
}
