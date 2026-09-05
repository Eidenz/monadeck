//! One mirrored monitor: a PipeWire capture feeding an OpenXR swapchain that is
//! shown as a grabbable quad layer, plus the laser → desktop-cursor mapping.
use std::sync::Mutex;
use std::time::Instant;

use anyhow::{anyhow, Result};
use ash::vk;
use ash::vk::Handle as _;
use openxr as xr;

use super::dmabuf::{self, Caps, Importer, Staging};
use super::pw::{Capture, Frame};
use crate::gfx::{cyl_layout, CylLayout};
use crate::mathx::{pose_compose, pose_invert, raycast, raycast_cylinder};

/// Arc of a fully curved screen (curve = 1), radians.
pub const MAX_CURVE_ANGLE: f32 = 1.5;

/// Default physical width of a mirrored screen, metres.
pub const DEFAULT_WIDTH_M: f32 = 1.35;

pub struct Swap {
    pub swapchain: xr::Swapchain<xr::Vulkan>,
    pub images: Vec<vk::Image>,
    pub px: (u32, u32),
}

pub struct ScreenPanel {
    pub name: String,
    pub detail: String,
    pub node_id: u32,
    /// Logical desktop rectangle (x, y, w, h) this screen covers.
    pub rect: (f64, f64, f64, f64),
    pub shown: bool,
    /// Placed in front of the head once (on first show).
    pub placed: bool,
    pub pose: xr::Posef,
    pub width_m: f32,
    /// 0 = flat quad; up to 1 = wraps `MAX_CURVE_ANGLE` around you (cylinder layer).
    pub curve: f32,
    /// Resize gesture in progress: (hand→head distance at start, width at start).
    pub resize_ref: Option<(f32, f32)>,
    /// Sized by hand (gesture/layout): the default-width setting leaves it alone.
    pub custom_size: bool,
    /// Layer opacity 0.2..=1 (needs XR_KHR_composition_layer_color_scale_bias).
    pub opacity: f32,
    /// Capture paused because nobody's looking at it.
    pub gaze_paused: bool,
    pub unseen_since: Option<Instant>,
    /// Chained onto the layer for opacity; lives here so the pointer stays valid.
    scale_bias: xr::sys::CompositionLayerColorScaleBiasKHR,
    pub capture: Option<Capture>,
    pub swap: Option<Swap>,
    /// At least one frame has been uploaded (the layer may be submitted).
    pub has_content: bool,
    pub frame_px: (u32, u32),
    staging: Option<Staging>,
    /// (hand index, controller→panel offset) while gripped.
    pub grab: Option<(usize, xr::Posef)>,
    pub frames: u64,
    pub last_error: Option<String>,
}

impl ScreenPanel {
    pub fn new(name: String, detail: String, node_id: u32, rect: (f64, f64, f64, f64)) -> Self {
        Self {
            name,
            detail,
            node_id,
            rect,
            shown: false,
            placed: false,
            pose: xr::Posef::IDENTITY,
            width_m: DEFAULT_WIDTH_M,
            curve: 0.0,
            resize_ref: None,
            custom_size: false,
            opacity: 1.0,
            gaze_paused: false,
            unseen_since: None,
            scale_bias: xr::sys::CompositionLayerColorScaleBiasKHR {
                ty: xr::sys::CompositionLayerColorScaleBiasKHR::TYPE,
                next: std::ptr::null(),
                color_scale: xr::Color4f { r: 1.0, g: 1.0, b: 1.0, a: 1.0 },
                color_bias: xr::Color4f { r: 0.0, g: 0.0, b: 0.0, a: 0.0 },
            },
            capture: None,
            swap: None,
            has_content: false,
            frame_px: (0, 0),
            staging: None,
            grab: None,
            frames: 0,
            last_error: None,
        }
    }

    pub fn size_m(&self) -> (f32, f32) {
        let aspect = if self.frame_px.0 > 0 && self.frame_px.1 > 0 {
            self.frame_px.1 as f32 / self.frame_px.0 as f32
        } else if self.rect.2 > 0.0 {
            (self.rect.3 / self.rect.2) as f32
        } else {
            9.0 / 16.0
        };
        (self.width_m, self.width_m * aspect)
    }

    /// Curved placement (None when flat or the runtime can't do cylinders).
    pub fn cyl(&self, curved_ok: bool) -> Option<CylLayout> {
        if !curved_ok || self.curve <= 0.01 {
            return None;
        }
        let (w, h) = self.size_m();
        let angle = self.curve.clamp(0.0, 1.0) * MAX_CURVE_ANGLE;
        let r = w / angle;
        Some(cyl_layout(&self.pose, r, r, 0.0, 0.0, w, h))
    }

    /// Hit-test the laser; (u, v, t) with (0,0) top-left of the screen image.
    pub fn hit(&self, aim: &xr::Posef, curved_ok: bool) -> Option<(f32, f32, f32)> {
        if !self.shown || !self.has_content {
            return None;
        }
        match self.cyl(curved_ok) {
            Some(l) => raycast_cylinder(aim, &l.pose, l.radius, l.central_angle, l.height),
            None => raycast(aim, &self.pose, self.size_m()),
        }
    }

    /// Desktop-logical cursor position for a (u, v) hit.
    pub fn desktop_pos(&self, u: f32, v: f32) -> (f64, f64) {
        (self.rect.0 + u as f64 * self.rect.2, self.rect.1 + v as f64 * self.rect.3)
    }

    pub fn start_grab(&mut self, hand: usize, aim: &xr::Posef) {
        self.grab = Some((hand, pose_compose(&pose_invert(aim), &self.pose)));
    }

    pub fn show(&mut self, caps: &Caps) {
        self.shown = true;
        match &self.capture {
            Some(c) => c.set_active(true),
            None => {
                self.capture = Some(Capture::start(format!("monadeck:{}", self.name), self.node_id, caps.formats.clone()));
            }
        }
    }

    pub fn hide(&mut self) {
        self.shown = false;
        self.grab = None;
        self.gaze_paused = false;
        self.unseen_since = None;
        if let Some(c) = &self.capture {
            c.set_active(false);
        }
    }

    /// Pause/resume the capture based on whether the head is turned toward the
    /// screen (frames keep flowing for ~2 s after looking away).
    pub fn gaze_update(&mut self, hmd: Option<&xr::Posef>, enabled: bool) {
        if !self.shown || !self.placed {
            return;
        }
        let looking = match (hmd, enabled) {
            (Some(h), true) => {
                let fwd = crate::mathx::normalize(crate::mathx::forward(h));
                let to = crate::mathx::normalize([
                    self.pose.position.x - h.position.x,
                    self.pose.position.y - h.position.y,
                    self.pose.position.z - h.position.z,
                ]);
                let cos = fwd[0] * to[0] + fwd[1] * to[1] + fwd[2] * to[2];
                cos > 0.25 // ~75° half-angle
            }
            _ => true,
        };
        if looking {
            self.unseen_since = None;
            if self.gaze_paused {
                self.gaze_paused = false;
                if let Some(c) = &self.capture {
                    c.set_active(true);
                }
            }
        } else if !self.gaze_paused {
            let since = *self.unseen_since.get_or_insert_with(Instant::now);
            if since.elapsed().as_secs_f32() > 2.0 {
                self.gaze_paused = true;
                if let Some(c) = &self.capture {
                    c.set_active(false);
                }
            }
        }
    }

    /// Refresh the opacity chain; returns the `next` pointer for the layer.
    fn opacity_next(&mut self, color_scale_ok: bool) -> *const std::ffi::c_void {
        if !color_scale_ok || self.opacity >= 0.995 {
            return std::ptr::null();
        }
        let a = self.opacity.clamp(0.05, 1.0);
        // Premultiplied: scale colour and alpha together.
        self.scale_bias.color_scale = xr::Color4f { r: a, g: a, b: a, a };
        &self.scale_bias as *const _ as *const std::ffi::c_void
    }

    /// Pull the newest captured frame into the swapchain (creating/resizing it
    /// as the stream dictates). Blocking on the GPU like the UI panels do.
    #[allow(clippy::too_many_arguments)]
    pub fn upload(
        &mut self,
        session: &xr::Session<xr::Vulkan>,
        device: &ash::Device,
        allocator: &Mutex<gpu_allocator::vulkan::Allocator>,
        importer: Option<&Importer>,
        caps: &Caps,
        cmd: vk::CommandBuffer,
        queue: vk::Queue,
        fence: vk::Fence,
    ) -> Result<()> {
        let Some(capture) = &self.capture else { return Ok(()) };
        let Some(frame) = capture.latest() else { return Ok(()) };
        let fmt = frame.format();
        if fmt.width == 0 || fmt.height == 0 {
            return Ok(());
        }
        if self.swap.as_ref().map_or(true, |s| s.px != (fmt.width, fmt.height)) {
            self.swap = None; // drop the old one first (frees its images)
            let swapchain = session.create_swapchain(&xr::SwapchainCreateInfo {
                create_flags: xr::SwapchainCreateFlags::EMPTY,
                usage_flags: xr::SwapchainUsageFlags::COLOR_ATTACHMENT | xr::SwapchainUsageFlags::TRANSFER_DST,
                format: caps.swap_format.as_raw() as _,
                sample_count: 1,
                width: fmt.width,
                height: fmt.height,
                face_count: 1,
                array_size: 1,
                mip_count: 1,
            })?;
            let images = swapchain.enumerate_images()?.into_iter().map(vk::Image::from_raw).collect();
            self.swap = Some(Swap { swapchain, images, px: (fmt.width, fmt.height) });
            self.frame_px = (fmt.width, fmt.height);
            self.has_content = false;
            log::info!("desktop: {} swapchain {}x{}", self.name, fmt.width, fmt.height);
        }
        let swap = self.swap.as_mut().ok_or_else(|| anyhow!("no swapchain"))?;
        let extent = vk::Extent2D { width: swap.px.0, height: swap.px.1 };

        let index = swap.swapchain.acquire_image()? as usize;
        swap.swapchain.wait_image(xr::Duration::INFINITE)?;
        let dst = swap.images[index];

        let mut imported = None;
        let result: Result<()> = (|| {
            unsafe {
                device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
                device.begin_command_buffer(
                    cmd,
                    &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )?;
            }
            dmabuf::cmd_transition(
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
            match &frame {
                Frame::Dmabuf(f) => {
                    let imp = importer.ok_or_else(|| anyhow!("DMA-BUF frame but import unsupported"))?;
                    let img = imp.import(f)?;
                    imp.cmd_blit(cmd, &img, dst, extent);
                    imported = Some(img);
                }
                Frame::Shm(f) => {
                    let need = (f.format.width as u64) * (f.format.height as u64) * 4;
                    Staging::ensure(&mut self.staging, device, allocator, need)?;
                    self.staging
                        .as_mut()
                        .ok_or_else(|| anyhow!("staging missing"))?
                        .cmd_upload(device, cmd, f, caps.swap_format, dst)?;
                }
            }
            dmabuf::cmd_transition(
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
            Ok(())
        })();
        if let (Some(img), Some(imp)) = (imported, importer) {
            imp.destroy(img);
        }
        swap.swapchain.release_image()?;
        match result {
            Ok(()) => {
                self.has_content = true;
                self.frames += 1;
                self.last_error = None;
            }
            Err(e) => {
                if self.last_error.as_deref() != Some(&e.to_string()) {
                    log::error!("desktop: {} frame upload failed: {e}", self.name);
                }
                self.last_error = Some(e.to_string());
            }
        }
        Ok(())
    }

    fn sub_image(swap: &Swap) -> xr::SwapchainSubImage<'_, xr::Vulkan> {
        xr::SwapchainSubImage::new().swapchain(&swap.swapchain).image_array_index(0).image_rect(xr::Rect2Di {
            offset: xr::Offset2Di { x: 0, y: 0 },
            extent: xr::Extent2Di { width: swap.px.0 as i32, height: swap.px.1 as i32 },
        })
    }

    /// Curved layer (when `curve` > 0 and supported).
    pub fn cylinder<'a>(
        &'a mut self,
        space: &'a xr::Space,
        curved_ok: bool,
        color_scale_ok: bool,
    ) -> Option<xr::CompositionLayerCylinderKHR<'a, xr::Vulkan>> {
        if !self.shown || !self.has_content {
            return None;
        }
        let l = self.cyl(curved_ok)?;
        let next = self.opacity_next(color_scale_ok);
        let swap = self.swap.as_ref()?;
        let mut c = xr::CompositionLayerCylinderKHR::new()
            .space(space)
            .eye_visibility(xr::EyeVisibility::BOTH)
            .sub_image(Self::sub_image(swap))
            .pose(l.pose)
            .radius(l.radius)
            .central_angle(l.central_angle)
            .aspect_ratio(swap.px.0 as f32 / swap.px.1 as f32);
        if !next.is_null() {
            c = c.layer_flags(xr::CompositionLayerFlags::BLEND_TEXTURE_SOURCE_ALPHA);
            // The safe wrapper is repr(transparent) over the sys struct.
            let raw: &mut xr::sys::CompositionLayerCylinderKHR =
                unsafe { &mut *(&mut c as *mut _ as *mut xr::sys::CompositionLayerCylinderKHR) };
            raw.next = next;
        }
        Some(c)
    }

    /// Flat layer (when not curved).
    pub fn quad<'a>(
        &'a mut self,
        space: &'a xr::Space,
        curved_ok: bool,
        color_scale_ok: bool,
    ) -> Option<xr::CompositionLayerQuad<'a, xr::Vulkan>> {
        if !self.shown || !self.has_content || self.cyl(curved_ok).is_some() {
            return None;
        }
        let next = self.opacity_next(color_scale_ok);
        let swap = self.swap.as_ref()?;
        let sub = xr::SwapchainSubImage::new().swapchain(&swap.swapchain).image_array_index(0).image_rect(
            xr::Rect2Di {
                offset: xr::Offset2Di { x: 0, y: 0 },
                extent: xr::Extent2Di { width: swap.px.0 as i32, height: swap.px.1 as i32 },
            },
        );
        let size = self.size_m();
        let mut q = xr::CompositionLayerQuad::new()
            .space(space)
            .eye_visibility(xr::EyeVisibility::BOTH)
            .sub_image(sub)
            .pose(self.pose)
            .size(xr::Extent2Df { width: size.0, height: size.1 });
        if !next.is_null() {
            q = q.layer_flags(xr::CompositionLayerFlags::BLEND_TEXTURE_SOURCE_ALPHA);
            let raw: &mut xr::sys::CompositionLayerQuad =
                unsafe { &mut *(&mut q as *mut _ as *mut xr::sys::CompositionLayerQuad) };
            raw.next = next;
        }
        Some(q)
    }

    pub fn destroy_gpu(&mut self, device: &ash::Device, allocator: &Mutex<gpu_allocator::vulkan::Allocator>) {
        if let Some(s) = self.staging.take() {
            s.destroy(device, allocator);
        }
        self.swap = None;
        self.has_content = false;
    }
}
