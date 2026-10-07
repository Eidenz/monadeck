//! The 3D layer: SteamVR's models of your controllers, trackers and base
//! stations, glove hands, a floor grid and the boundary, like SteamVR draws
//! them around its dashboard. Two projection layers: the world (grid, base
//! stations, trackers, the boundary) under Monadeck's panels, the hands
//! (controllers, gloves) over them, since they're nearly always the closer.
pub mod math;
pub mod models;
mod gpu;
pub mod selftest;
pub mod walls;

use std::collections::HashMap;

use anyhow::Result;
use ash::vk;
use ash::vk::Handle as _;
use openxr as xr;

use crate::mathx::{pose_compose, pose_invert, quat_from_axis_angle, quat_rotate, q_mul};
use gpu::{Draw, Gpu, Mesh, Mode, Target, Texture};
use math::Mat4;
use models::{Input, Motion, Vertex};
pub use walls::Walls;

/// The rim and grid colour (linear): Monadeck's teal.
const TEAL: [f32; 3] = [0.05, 0.74, 0.63];
/// Glove hands' joints and bones.
const HAND_TINT: [f32; 3] = [0.62, 0.68, 0.74];
/// The boundary's outline on the floor: lighter than the grid it lies on.
const LINE_TINT: [f32; 3] = [0.55, 0.95, 0.88];
/// Markers while a boundary is drawn.
pub const MARKER_TINT: [f32; 3] = [0.85, 0.9, 0.95];
/// The projection layers render at this share of the runtime's recommended
/// size: plenty for models a few hundred pixels across.
const RENDER_SCALE: f32 = 0.6;

/// What moves a controller model's parts.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Buttons {
    pub trigger: f32,
    pub a: bool,
    pub b: bool,
    pub system: bool,
    pub stick: (f32, f32),
    pub stick_click: bool,
}

/// One thing in the scene, in the eye views' space.
pub enum Item {
    /// A SteamVR model at `pose` (its model origin).
    Model { name: String, pose: xr::Posef, buttons: Option<Buttons>, alpha: f32 },
    /// A controller by its grip pose: the model's own grip frame (`openxr_grip`,
    /// else `grip`) puts it in the hand.
    Controller { name: String, grip: xr::Posef, buttons: Buttons },
    /// A glove hand: XR_EXT_hand_tracking's joints.
    Hand(Box<xr::HandJointLocations>),
    /// The floor grid: STAGE's pose, and where you stand in it.
    Grid { stage: xr::Posef, at: [f32; 2] },
    /// The play area's boundary (one per frame is drawn).
    Walls(Box<Walls>),
    /// A ball, and a rod between two points (markers).
    Dot { at: [f32; 3], radius: f32, tint: [f32; 3] },
    Rod { from: [f32; 3], to: [f32; 3], radius: f32, tint: [f32; 3] },
}

enum Slot {
    Loading,
    Ready(GpuModel),
    Missing,
}

struct GpuPart {
    mesh: Mesh,
    texture: Option<usize>,
    motion: Option<Motion>,
    /// The part's own frame (tilts a stick about its own axes).
    frame: Option<[f32; 4]>,
}

struct GpuModel {
    parts: Vec<GpuPart>,
    textures: Vec<Texture>,
    grip: Option<xr::Posef>,
}

/// A projection layer's swapchain (both eyes as array layers) and targets.
struct Layer {
    swapchain: xr::Swapchain<xr::Vulkan>,
    target: Target,
    /// Drawn this frame: submit it.
    drawn: bool,
    views: [(xr::Posef, xr::Fovf); 2],
}

pub struct Scene {
    gpu: Gpu,
    format: vk::Format,
    loader: models::Loader,
    models: HashMap<String, Slot>,
    sphere: Mesh,
    cylinder: Mesh,
    grid: Mesh,
    /// The boundary's, rebuilt each frame it shows.
    wall_mesh: Option<Mesh>,
    line_mesh: Option<Mesh>,
    wall_data: walls::MeshData,
    line_data: walls::MeshData,
    world: Option<Layer>,
    hands: Option<Layer>,
    extent: vk::Extent2D,
    /// Some model came back missing: SteamVR (or that model) isn't installed.
    pub missing: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Which {
    World,
    Hands,
}

impl Scene {
    /// `recommended`: the runtime's per-eye size.
    pub fn new(
        device: &ash::Device,
        allocator: std::sync::Arc<std::sync::Mutex<gpu_allocator::vulkan::Allocator>>,
        format: vk::Format,
        recommended: (u32, u32),
    ) -> Result<Self> {
        let gpu = Gpu::new(device, allocator, format)?;
        let (sv, si) = sphere(12, 8);
        let (cv, ci) = cylinder(12);
        let (gv, gi) = grid_quad(12.0);
        let extent = vk::Extent2D {
            width: ((recommended.0 as f32 * RENDER_SCALE) as u32).max(256),
            height: ((recommended.1 as f32 * RENDER_SCALE) as u32).max(256),
        };
        Ok(Self {
            sphere: gpu.mesh(&sv, &si)?,
            cylinder: gpu.mesh(&cv, &ci)?,
            grid: gpu.mesh(&gv, &gi)?,
            wall_mesh: None,
            line_mesh: None,
            wall_data: Default::default(),
            line_data: Default::default(),
            gpu,
            format,
            loader: models::Loader::spawn(),
            models: HashMap::new(),
            world: None,
            hands: None,
            extent,
            missing: Vec::new(),
        })
    }

    /// Ask for the models `items` need, and upload any that arrived.
    fn prepare(&mut self, items: &[&Item], cmd: vk::CommandBuffer, queue: vk::Queue, fence: vk::Fence) -> Result<()> {
        self.gpu.ensure_white(cmd, queue, fence)?;
        for item in items {
            let name = match item {
                Item::Model { name, .. } | Item::Controller { name, .. } => name,
                _ => continue,
            };
            if !self.models.contains_key(name) {
                self.models.insert(name.clone(), Slot::Loading);
                self.loader.request(name);
            }
        }
        while let Some((name, result)) = self.loader.try_recv() {
            let slot = match result {
                Ok(data) => {
                    let mut textures = Vec::new();
                    for img in &data.textures {
                        textures.push(self.gpu.texture(img, cmd, queue, fence)?);
                    }
                    let mut parts = Vec::new();
                    for p in &data.parts {
                        parts.push(GpuPart {
                            mesh: self.gpu.mesh(&p.vertices, &p.indices)?,
                            texture: p.texture,
                            motion: p.motion.clone(),
                            frame: data.frames.get(&p.component).map(|f| f.1),
                        });
                    }
                    let grip = data.frames.get("openxr_grip").or_else(|| data.frames.get("grip")).map(|(p, q)| math::pose(*p, *q));
                    log::info!("scene: model {name} ready ({} parts, {} textures)", parts.len(), textures.len());
                    Slot::Ready(GpuModel { parts, textures, grip })
                }
                Err(e) => {
                    log::warn!("scene: {e}");
                    self.missing.push(name.clone());
                    Slot::Missing
                }
            };
            self.models.insert(name, slot);
        }
        Ok(())
    }

    fn make_layer(&self, session: &xr::Session<xr::Vulkan>) -> Result<Layer> {
        let swapchain = session.create_swapchain(&xr::SwapchainCreateInfo {
            create_flags: xr::SwapchainCreateFlags::EMPTY,
            usage_flags: xr::SwapchainUsageFlags::COLOR_ATTACHMENT | xr::SwapchainUsageFlags::SAMPLED,
            format: self.format.as_raw() as _,
            sample_count: 1,
            width: self.extent.width,
            height: self.extent.height,
            face_count: 1,
            array_size: 2,
            mip_count: 1,
        })?;
        let images: Vec<vk::Image> = swapchain.enumerate_images()?.into_iter().map(vk::Image::from_raw).collect();
        let target = self.gpu.target(&images, self.format, self.extent, 2)?;
        Ok(Layer { swapchain, target, drawn: false, views: [(xr::Posef::IDENTITY, xr::Fovf::default()); 2] })
    }

    /// Render both layers for this frame (a layer with nothing in it is skipped,
    /// and not submitted).
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        session: &xr::Session<xr::Vulkan>,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        queue: vk::Queue,
        fence: vk::Fence,
        views: &[xr::View],
        world: &[Item],
        hands: &[Item],
    ) -> Result<()> {
        if let Some(l) = &mut self.world {
            l.drawn = false;
        }
        if let Some(l) = &mut self.hands {
            l.drawn = false;
        }
        if views.len() < 2 || (world.is_empty() && hands.is_empty()) {
            return Ok(());
        }
        // xrEndFrame refuses a projection view whose rotation isn't a unit
        // quaternion (and the overlay with it).
        let unit = |v: &xr::View| {
            let q = v.pose.orientation;
            ((q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w) - 1.0).abs() < 0.01
        };
        if !views.iter().take(2).all(unit) {
            return Ok(());
        }
        let all: Vec<&Item> = world.iter().chain(hands).collect();
        self.prepare(&all, cmd, queue, fence)?;
        self.prepare_walls(world)?;
        for (which, items) in [(Which::World, world), (Which::Hands, hands)] {
            if items.is_empty() {
                continue;
            }
            let draws_ready = items.iter().any(|i| self.drawable(i));
            if !draws_ready {
                continue;
            }
            let slot = match which {
                Which::World => &mut self.world,
                Which::Hands => &mut self.hands,
            };
            if slot.is_none() {
                let layer = self.make_layer(session)?;
                match which {
                    Which::World => self.world = Some(layer),
                    Which::Hands => self.hands = Some(layer),
                }
            }
            let mut layer = match which {
                Which::World => self.world.take(),
                Which::Hands => self.hands.take(),
            }
            .expect("layer just made");
            let r = self.draw_layer(&mut layer, device, cmd, queue, fence, views, items);
            match which {
                Which::World => self.world = Some(layer),
                Which::Hands => self.hands = Some(layer),
            }
            r?;
        }
        Ok(())
    }

    /// The boundary's meshes for this frame.
    fn prepare_walls(&mut self, items: &[Item]) -> Result<()> {
        let Some(w) = items.iter().find_map(|i| if let Item::Walls(w) = i { Some(w) } else { None }) else { return Ok(()) };
        walls::build(w, &mut self.wall_data, &mut self.line_data);
        self.gpu.refill(&mut self.wall_mesh, &self.wall_data.0, &self.wall_data.1)?;
        self.gpu.refill(&mut self.line_mesh, &self.line_data.0, &self.line_data.1)?;
        Ok(())
    }

    fn drawable(&self, item: &Item) -> bool {
        match item {
            Item::Model { name, .. } | Item::Controller { name, .. } => matches!(self.models.get(name), Some(Slot::Ready(_))),
            _ => true,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_layer(
        &self,
        layer: &mut Layer,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        queue: vk::Queue,
        fence: vk::Fence,
        views: &[xr::View],
        items: &[Item],
    ) -> Result<()> {
        let index = layer.swapchain.acquire_image()? as usize;
        layer.swapchain.wait_image(xr::Duration::INFINITE)?;
        unsafe {
            device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
            device.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT))?;
        }
        let draws = self.draws(items);
        for (eye, v) in views.iter().take(2).enumerate() {
            let view = math::view(&v.pose);
            let proj = math::projection(&v.fov, 0.03, 100.0);
            self.gpu.record(cmd, layer.target.framebuffers[index][eye], layer.target.extent, &view, &proj, &draws);
            layer.views[eye] = (v.pose, v.fov);
        }
        unsafe {
            device.end_command_buffer(cmd)?;
            let cmds = [cmd];
            device.queue_submit(queue, &[vk::SubmitInfo::default().command_buffers(&cmds)], fence)?;
            device.wait_for_fences(&[fence], true, u64::MAX)?;
            device.reset_fences(&[fence])?;
        }
        layer.swapchain.release_image()?;
        layer.drawn = true;
        Ok(())
    }

    /// The projection views of a layer drawn this frame, for its composition layer.
    pub fn views(&self, which: Which) -> Option<[xr::CompositionLayerProjectionView<'_, xr::Vulkan>; 2]> {
        let layer = match which {
            Which::World => self.world.as_ref(),
            Which::Hands => self.hands.as_ref(),
        }?;
        if !layer.drawn {
            return None;
        }
        let rect = xr::Rect2Di {
            offset: xr::Offset2Di { x: 0, y: 0 },
            extent: xr::Extent2Di { width: self.extent.width as i32, height: self.extent.height as i32 },
        };
        let view = |eye: usize| {
            xr::CompositionLayerProjectionView::new()
                .pose(layer.views[eye].0)
                .fov(layer.views[eye].1)
                .sub_image(xr::SwapchainSubImage::new().swapchain(&layer.swapchain).image_array_index(eye as u32).image_rect(rect))
        };
        Some([view(0), view(1)])
    }

    /// The draw list for `items`.
    fn draws<'a>(&'a self, items: &[Item]) -> Vec<Draw<'a>> {
        let mut out = Vec::new();
        for item in items {
            match item {
                Item::Grid { stage, at } => {
                    // The quad moves in whole metres so its lines stay put on the floor.
                    let snap = [at[0].round(), at[1].round()];
                    let model = math::mul(&math::from_pose(stage), &math::from_trs([snap[0], 0.0, snap[1]], [0.0, 0.0, 0.0, 1.0], [1.0; 3]));
                    out.push(Draw {
                        mesh: &self.grid,
                        texture: None,
                        model,
                        tint: TEAL,
                        alpha: 0.4,
                        mode: Mode::Grid,
                        center: [at[0] - snap[0], at[1] - snap[1]],
                    });
                }
                Item::Hand(joints) => self.hand_draws(joints, &mut out),
                Item::Walls(w) => {
                    let model = math::from_pose(&w.room);
                    let draw = |mesh, mode, tint| Draw { mesh, texture: None, model, tint, alpha: 1.0, mode, center: [walls::HEIGHT, 0.0] };
                    if let Some(m) = &self.line_mesh {
                        out.push(draw(m, Mode::Line, LINE_TINT));
                    }
                    if let Some(m) = &self.wall_mesh {
                        out.push(draw(m, Mode::Wall, TEAL));
                    }
                }
                Item::Dot { at, radius, tint } => out.push(Draw {
                    mesh: &self.sphere,
                    texture: None,
                    model: math::from_trs(*at, [0.0, 0.0, 0.0, 1.0], [*radius; 3]),
                    tint: *tint,
                    alpha: 0.9,
                    mode: Mode::Tint,
                    center: [0.0; 2],
                }),
                Item::Rod { from, to, radius, tint } => out.extend(self.rod(*from, *to, *radius, *tint, 0.9)),
                Item::Model { name, pose, buttons, alpha } => self.model_draws(name, pose, buttons.as_ref(), *alpha, &mut out),
                Item::Controller { name, grip, buttons } => {
                    if let Some(Slot::Ready(m)) = self.models.get(name) {
                        let origin = match &m.grip {
                            Some(g) => pose_compose(grip, &pose_invert(g)),
                            None => *grip,
                        };
                        self.model_draws(name, &origin, Some(buttons), 0.92, &mut out);
                    }
                }
            }
        }
        out
    }

    fn model_draws<'a>(&'a self, name: &str, pose: &xr::Posef, buttons: Option<&Buttons>, alpha: f32, out: &mut Vec<Draw<'a>>) {
        let Some(Slot::Ready(m)) = self.models.get(name) else { return };
        let base = math::from_pose(pose);
        for part in &m.parts {
            let local = match (&part.motion, buttons) {
                (Some(motion), Some(b)) => animate(motion, part.frame, b),
                _ => math::IDENTITY,
            };
            out.push(Draw {
                mesh: &part.mesh,
                texture: part.texture.and_then(|i| m.textures.get(i)),
                model: math::mul(&base, &local),
                tint: TEAL,
                alpha,
                mode: if part.texture.is_some() { Mode::Textured } else { Mode::Tint },
                center: [0.0; 2],
            });
        }
    }

    /// Joints as spheres, bones as rods between them.
    fn hand_draws<'a>(&'a self, joints: &xr::HandJointLocations, out: &mut Vec<Draw<'a>>) {
        let valid = |j: &xr::HandJointLocation| j.location_flags.contains(xr::SpaceLocationFlags::POSITION_VALID);
        for j in joints.iter().filter(|j| valid(j)) {
            let r = j.radius.max(0.004);
            out.push(Draw {
                mesh: &self.sphere,
                texture: None,
                model: math::from_trs(math::pos(&j.pose), math::quat(&j.pose), [r, r, r]),
                tint: HAND_TINT,
                alpha: 0.9,
                mode: Mode::Tint,
                center: [0.0; 2],
            });
        }
        for (a, b) in HAND_BONES {
            let (ja, jb) = (&joints[a], &joints[b]);
            if !valid(ja) || !valid(jb) {
                continue;
            }
            let r = ja.radius.min(jb.radius).max(0.003) * 0.75;
            out.extend(self.rod(math::pos(&ja.pose), math::pos(&jb.pose), r, HAND_TINT, 0.9));
        }
    }

    /// A rod of radius `r` from `a` to `b`.
    fn rod(&self, a: [f32; 3], b: [f32; 3], r: f32, tint: [f32; 3], alpha: f32) -> Option<Draw<'_>> {
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if len < 1e-4 {
            return None;
        }
        // The rod's +Y onto the line.
        let q = from_to([0.0, 1.0, 0.0], [d[0] / len, d[1] / len, d[2] / len]);
        Some(Draw { mesh: &self.cylinder, texture: None, model: math::from_trs(a, q, [r, len, r]), tint, alpha, mode: Mode::Tint, center: [0.0; 2] })
    }
}

/// A hand tracker; with `unobstructed_only`, fed by cameras and gloves only
/// (XR_EXT_hand_tracking_data_source), so a controller's finger tracking (an
/// Index) never comes back as a hand.
pub fn hand_tracker(session: &xr::Session<xr::Vulkan>, instance: &xr::Instance, hand: xr::Hand, unobstructed_only: bool) -> xr::Result<xr::HandTracker> {
    use xr::sys::Handle as _;
    if !unobstructed_only {
        return session.create_hand_tracker(hand);
    }
    let fp = instance.exts().ext_hand_tracking.as_ref().ok_or(xr::sys::Result::ERROR_EXTENSION_NOT_PRESENT)?;
    let mut sources = [xr::sys::HandTrackingDataSourceEXT::UNOBSTRUCTED];
    let source_info = xr::sys::HandTrackingDataSourceInfoEXT {
        ty: xr::sys::HandTrackingDataSourceInfoEXT::TYPE,
        next: std::ptr::null(),
        requested_data_source_count: sources.len() as u32,
        requested_data_sources: sources.as_mut_ptr(),
    };
    let info = xr::sys::HandTrackerCreateInfoEXT {
        ty: xr::sys::HandTrackerCreateInfoEXT::TYPE,
        next: (&source_info as *const xr::sys::HandTrackingDataSourceInfoEXT).cast(),
        hand,
        hand_joint_set: xr::sys::HandJointSetEXT::DEFAULT,
    };
    let mut out = xr::sys::HandTrackerEXT::NULL;
    let result = unsafe { (fp.create_hand_tracker)(session.as_raw(), &info, &mut out) };
    if result.into_raw() < 0 {
        return Err(result);
    }
    // SAFETY: just created on this session.
    Ok(unsafe { xr::HandTracker::from_raw(session, out) })
}

/// A drawn layer's composition layer (premultiplied alpha), in `space`.
pub fn projection_layer<'a>(
    space: &'a xr::Space,
    views: &'a [xr::CompositionLayerProjectionView<'a, xr::Vulkan>; 2],
) -> xr::CompositionLayerProjection<'a, xr::Vulkan> {
    xr::CompositionLayerProjection::new().space(space).layer_flags(xr::CompositionLayerFlags::BLEND_TEXTURE_SOURCE_ALPHA).views(views)
}

/// XR_EXT_hand_tracking's joint pairs: wrist to each finger's base, then along it.
const HAND_BONES: [(usize, usize); 24] = [
    (1, 2), (2, 3), (3, 4), (4, 5),
    (1, 6), (6, 7), (7, 8), (8, 9), (9, 10),
    (1, 11), (11, 12), (12, 13), (13, 14), (14, 15),
    (1, 16), (16, 17), (17, 18), (18, 19), (19, 20),
    (1, 21), (21, 22), (22, 23), (23, 24), (24, 25),
];

fn from_to(a: [f32; 3], b: [f32; 3]) -> [f32; 4] {
    let c = crate::mathx::cross(a, b);
    let d = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    if d < -0.9999 {
        return [1.0, 0.0, 0.0, 0.0];
    }
    let q = [c[0], c[1], c[2], 1.0 + d];
    let l = q.iter().map(|x| x * x).sum::<f32>().sqrt();
    q.map(|x| x / l)
}

/// A moving part's transform in model space.
fn animate(motion: &Motion, frame: Option<[f32; 4]>, b: &Buttons) -> Mat4 {
    let pressed = |i: Input| match i {
        Input::Trigger => b.trigger,
        Input::A => b.a as u8 as f32,
        Input::B => b.b as u8 as f32,
        Input::System => b.system as u8 as f32,
        Input::Stick => b.stick_click as u8 as f32,
    };
    let about = |pivot: [f32; 3], q: [f32; 4]| {
        // T(pivot) · R · T(-pivot)
        let t = quat_rotate(q, [-pivot[0], -pivot[1], -pivot[2]]);
        math::from_trs([t[0] + pivot[0], t[1] + pivot[1], t[2] + pivot[2]], q, [1.0; 3])
    };
    match motion {
        Motion::Rotate { input, pivot, axis, from, to } => {
            let deg = from + (to - from) * pressed(*input).clamp(0.0, 1.0);
            about(*pivot, quat_from_axis_angle(*axis, deg.to_radians()))
        }
        Motion::Translate { input, axis, from, to } => {
            let s = from + (to - from) * pressed(*input).clamp(0.0, 1.0);
            math::from_trs([axis[0] * s, axis[1] * s, axis[2] * s], [0.0, 0.0, 0.0, 1.0], [1.0; 3])
        }
        Motion::Joystick { center, tilt, press } => {
            // Tilt about the stick's own axes: forward pushes its top away.
            let f = frame.unwrap_or([0.0, 0.0, 0.0, 1.0]);
            let x_axis = quat_rotate(f, [1.0, 0.0, 0.0]);
            let z_axis = quat_rotate(f, [0.0, 0.0, 1.0]);
            let q = q_mul(
                quat_from_axis_angle(x_axis, (-b.stick.1 * tilt).to_radians()),
                quat_from_axis_angle(z_axis, (-b.stick.0 * tilt).to_radians()),
            );
            let mut m = about(*center, q);
            if b.stick_click {
                m[12] += press[0];
                m[13] += press[1];
                m[14] += press[2];
            }
            m
        }
    }
}

// --- Shapes ----------------------------------------------------------------------

/// A unit sphere (outward faces counter-clockwise).
fn sphere(segments: u32, rings: u32) -> (Vec<Vertex>, Vec<u32>) {
    let mut v = Vec::new();
    for r in 0..=rings {
        let phi = std::f32::consts::PI * r as f32 / rings as f32;
        for s in 0..=segments {
            let theta = std::f32::consts::TAU * s as f32 / segments as f32;
            let p = [phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin()];
            v.push(Vertex { pos: p, nrm: p, uv: [0.0; 2] });
        }
    }
    let mut i = Vec::new();
    let w = segments + 1;
    for r in 0..rings {
        for s in 0..segments {
            let (a, b, c, d) = (r * w + s, r * w + s + 1, (r + 1) * w + s, (r + 1) * w + s + 1);
            i.extend([a, b, c, b, d, c]);
        }
    }
    (v, i)
}

/// A unit-radius rod from y = 0 to y = 1, open ended (the joints cap it).
fn cylinder(segments: u32) -> (Vec<Vertex>, Vec<u32>) {
    let mut v = Vec::new();
    for s in 0..=segments {
        let theta = std::f32::consts::TAU * s as f32 / segments as f32;
        let n = [theta.cos(), 0.0, theta.sin()];
        v.push(Vertex { pos: [n[0], 0.0, n[2]], nrm: n, uv: [0.0; 2] });
        v.push(Vertex { pos: [n[0], 1.0, n[2]], nrm: n, uv: [0.0; 2] });
    }
    let mut i = Vec::new();
    for s in 0..segments {
        let (a, b, c, d) = (2 * s, 2 * s + 1, 2 * s + 2, 2 * s + 3);
        i.extend([a, b, c, b, d, c]);
    }
    (v, i)
}

/// The floor: `half`·2 metres square at y = 0, facing up; UVs are metres.
fn grid_quad(half: f32) -> (Vec<Vertex>, Vec<u32>) {
    let corner = |x: f32, z: f32| Vertex { pos: [x, 0.0, z], nrm: [0.0, 1.0, 0.0], uv: [x, z] };
    (vec![corner(-half, -half), corner(-half, half), corner(half, half), corner(half, -half)], vec![0, 1, 2, 0, 2, 3])
}

/// A model's name for a controller in a hand, by the bindings' controller
/// type (`knuckles`, `oculus_touch`). Touch-style controllers come with
/// standalone headsets: WiVRn names the system after the headset ("Meta
/// Quest 3 on WiVRn"), which picks the model; unknown ones look like a Quest 2's.
pub fn controller_model(ty: &str, right: bool, system: &str) -> Option<String> {
    let side = if right { "right" } else { "left" };
    match ty {
        "knuckles" => Some(format!("{{indexcontroller}}valve_controller_knu_1_0_{side}")),
        "oculus_touch" => {
            let s = system.to_ascii_lowercase();
            let model = if s.contains("quest 3") {
                "oculus_quest_plus_controller"
            } else if s.contains("quest pro") {
                "oculus_quest_pro_controller"
            } else if s.contains("quest 2") {
                "oculus_quest2_controller"
            } else if s.contains("quest") {
                "oculus_quest_controller"
            } else if s.contains("pico 4") || s.contains("pico4") {
                "{vrlink}pico_4_controller"
            } else if s.contains("focus 3") || s.contains("focus3") {
                "{vrlink}vive_focus3_controller"
            } else {
                "oculus_quest2_controller"
            };
            Some(format!("{model}_{side}"))
        }
        "vive_controller" => Some("vr_controller_vive_1_5".into()),
        _ => None,
    }
}

/// A tracker's model by the name Monado gives it, None when it isn't one.
pub fn tracker_model(name: &str) -> Option<&'static str> {
    let n = name.to_ascii_lowercase();
    // Eye, face and hand tracking devices are sensors, not things to draw.
    if !n.contains("tracker") || ["eye", "face", "hand"].iter().any(|w| n.contains(w)) {
        return None;
    }
    Some(if n.contains("tundra") {
        "{tundra_labs}tundra_tracker"
    } else if n.contains("3.0") {
        "{htc}vr_tracker_vive_3_0"
    } else if n.contains("vive") || n.contains("htc") {
        "{htc}vr_tracker_vive_1_0"
    } else {
        "generic_tracker"
    })
}

pub const BASE_STATION_MODEL: &str = "lh_basestation_valve_gen2";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_wind_outward_counter_clockwise() {
        // A sphere's triangle normal agrees with its vertex normals.
        let (v, i) = sphere(12, 8);
        let tri = &i[12 * 6..12 * 6 + 3];
        let (a, b, c) = (v[tri[0] as usize].pos, v[tri[1] as usize].pos, v[tri[2] as usize].pos);
        let n = crate::mathx::cross([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        let out = v[tri[0] as usize].nrm;
        assert!(n[0] * out[0] + n[1] * out[1] + n[2] * out[2] > 0.0);
        // The floor faces up.
        let (g, gi) = grid_quad(1.0);
        let (a, b, c) = (g[gi[0] as usize].pos, g[gi[1] as usize].pos, g[gi[2] as usize].pos);
        let n = crate::mathx::cross([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        assert!(n[1] > 0.0);
        // The rod's sides face out.
        let (cv, ci) = cylinder(12);
        let (a, b, c) = (cv[ci[0] as usize].pos, cv[ci[1] as usize].pos, cv[ci[2] as usize].pos);
        let n = crate::mathx::cross([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        let out = cv[ci[0] as usize].nrm;
        assert!(n[0] * out[0] + n[1] * out[1] + n[2] * out[2] > 0.0);
    }

    #[test]
    fn trackers_by_name() {
        assert_eq!(tracker_model("Tundra Tracker"), Some("{tundra_labs}tundra_tracker"));
        assert_eq!(tracker_model("VIVE Tracker 3.0 MV"), Some("{htc}vr_tracker_vive_3_0"));
        assert_eq!(tracker_model("Valve Knuckles Left"), None);
        assert_eq!(tracker_model("UDCAP Glove Left"), None);
        // WiVRn's: body trackers yes, its eye and face sensors no.
        assert_eq!(tracker_model("WiVRn Generic Tracker #1"), Some("generic_tracker"));
        assert_eq!(tracker_model("SlimeVR Virtual Tracker"), Some("generic_tracker"));
        assert_eq!(tracker_model("WiVRn Eye Tracker"), None);
        assert_eq!(tracker_model("WiVRn FB v2 Face Tracker"), None);
    }

    #[test]
    fn standalone_controllers_by_headset() {
        let m = |sys: &str| controller_model("oculus_touch", false, sys).unwrap();
        assert_eq!(m("Meta Quest 3 on WiVRn"), "oculus_quest_plus_controller_left");
        assert_eq!(m("Meta Quest 3S on WiVRn"), "oculus_quest_plus_controller_left");
        assert_eq!(m("Meta Quest Pro on WiVRn"), "oculus_quest_pro_controller_left");
        assert_eq!(m("Oculus Quest 2 on WiVRn"), "oculus_quest2_controller_left");
        assert_eq!(m("Oculus Quest on WiVRn"), "oculus_quest_controller_left");
        assert_eq!(m("Pico 4 Ultra on WiVRn"), "{vrlink}pico_4_controller_left");
        assert_eq!(m("WiVRn"), "oculus_quest2_controller_left");
        assert_eq!(controller_model("knuckles", true, "").as_deref(), Some("{indexcontroller}valve_controller_knu_1_0_right"));
    }
}
