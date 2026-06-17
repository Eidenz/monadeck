// Vulkan + egui rendering plumbing for the overlay's composition-layer panels.
// Ported (and trimmed to a single panel + laser) from monado-frame.
use std::sync::{Arc, Mutex};

use anyhow::Result;
use ash::vk;
use ash::vk::Handle as _;
use openxr as xr;

use crate::mathx::{cross, forward, normalize, qf, quat_from_axes, quat_rotate, quatf, vec3f};

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
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
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

/// Cylinder placement derived from the panel's flat anchor: `(pose, central_angle,
/// height)`. The axis sits `radius` toward the viewer from the panel centre, so
/// the panel's current centre + facing stay put while the surface curves around
/// it. Shared by the layer build and the hit-test so they always agree.
pub fn cylinder_params(p: &PanelGfx, radius: f32) -> (xr::Posef, f32, f32) {
    let z = quat_rotate(qf(&p.pose.orientation), [0.0, 0.0, 1.0]); // +Z = toward viewer
    let pos = [
        p.pose.position.x + z[0] * radius,
        p.pose.position.y + z[1] * radius,
        p.pose.position.z + z[2] * radius,
    ];
    let pose = xr::Posef { orientation: p.pose.orientation, position: vec3f(pos) };
    let central_angle = (p.size_m.0 / radius).min(std::f32::consts::PI * 0.9);
    (pose, central_angle, p.size_m.1)
}

pub fn cylinder_layer<'a>(
    p: &'a PanelGfx,
    space: &'a xr::Space,
    radius: f32,
) -> xr::CompositionLayerCylinderKHR<'a, xr::Vulkan> {
    let (pose, central_angle, _h) = cylinder_params(p, radius);
    let sub = xr::SwapchainSubImage::new().swapchain(&p.swapchain).image_array_index(0).image_rect(
        xr::Rect2Di {
            offset: xr::Offset2Di { x: 0, y: 0 },
            extent: xr::Extent2Di { width: p.px.0 as i32, height: p.px.1 as i32 },
        },
    );
    xr::CompositionLayerCylinderKHR::new()
        .space(space)
        .eye_visibility(xr::EyeVisibility::BOTH)
        .sub_image(sub)
        .pose(pose)
        .radius(radius)
        .central_angle(central_angle)
        .aspect_ratio(p.px.0 as f32 / p.px.1 as f32)
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

/// Fill the laser texture with the accent colour (called per shown frame).
pub fn fill_laser(
    laser: &mut Laser,
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    queue: vk::Queue,
    fence: vk::Fence,
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
        let color = vk::ClearColorValue { float32: [0.25, 0.88, 0.81, 1.0] };
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

/// A thin quad from the controller to the hit point, billboarded toward the HMD.
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
        .size(xr::Extent2Df { width: 0.006, height: dist })
}
