//! The 3D layer's Vulkan side: one pipeline (premultiplied alpha, depth
//! tested, back faces culled), meshes in host-visible buffers, sRGB
//! textures, and render targets over swapchain images (one array layer per
//! eye) or an offscreen image (the self-test).
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator};
use gpu_allocator::MemoryLocation;

use super::math::{self, Mat4};
use super::models::Vertex;

static VERT: &[u8] = include_bytes!("../../assets/shaders/scene.vert.spv");
static FRAG: &[u8] = include_bytes!("../../assets/shaders/scene.frag.spv");

pub const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;

/// The push constants (128 bytes, the guaranteed minimum): see scene.vert.
#[repr(C)]
#[derive(Clone, Copy)]
struct Push {
    mvp: Mat4,
    n0: [f32; 4],
    n1: [f32; 4],
    n2: [f32; 4],
    params: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Textured = 0,
    Tint = 1,
    Grid = 2,
}

pub struct Mesh {
    vbuf: vk::Buffer,
    ibuf: vk::Buffer,
    count: u32,
    _allocs: [Allocation; 2],
}

pub struct Texture {
    _image: vk::Image,
    _view: vk::ImageView,
    set: vk::DescriptorSet,
    _alloc: Allocation,
}

/// One thing to draw: a mesh at `model` (in the eye views' space).
pub struct Draw<'a> {
    pub mesh: &'a Mesh,
    pub texture: Option<&'a Texture>,
    pub model: Mat4,
    pub tint: [f32; 3],
    pub alpha: f32,
    pub mode: Mode,
    /// Grid only: where it fades out from, in the mesh's floor coordinates.
    pub center: [f32; 2],
}

pub struct Gpu {
    device: ash::Device,
    allocator: Arc<Mutex<Allocator>>,
    pub render_pass: vk::RenderPass,
    set_layout: vk::DescriptorSetLayout,
    layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    sampler: vk::Sampler,
    pool: vk::DescriptorPool,
    white: Option<Texture>,
}

fn alloc(allocator: &Arc<Mutex<Allocator>>, name: &str, requirements: vk::MemoryRequirements, location: MemoryLocation, linear: bool) -> Result<Allocation> {
    allocator
        .lock()
        .map_err(|_| anyhow!("allocator poisoned"))?
        .allocate(&AllocationCreateDesc { name, requirements, location, linear, allocation_scheme: AllocationScheme::GpuAllocatorManaged })
        .map_err(|e| anyhow!("{name}: {e}"))
}

/// Memory for the self-test's offscreen image (`host` false) and readback buffer.
pub(super) fn alloc_for_selftest(allocator: &Arc<Mutex<Allocator>>, requirements: vk::MemoryRequirements, host: bool) -> Result<Allocation> {
    let location = if host { MemoryLocation::GpuToCpu } else { MemoryLocation::GpuOnly };
    alloc(allocator, "scene-selftest", requirements, location, host)
}

impl Gpu {
    /// `format`: the colour targets' (the swapchain's, or the self-test's image).
    pub fn new(device: &ash::Device, allocator: Arc<Mutex<Allocator>>, format: vk::Format) -> Result<Self> {
        let device = device.clone();
        unsafe {
            let attachments = [
                vk::AttachmentDescription::default()
                    .format(format)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .load_op(vk::AttachmentLoadOp::CLEAR)
                    .store_op(vk::AttachmentStoreOp::STORE)
                    .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                    .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                    .initial_layout(vk::ImageLayout::UNDEFINED)
                    .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL),
                vk::AttachmentDescription::default()
                    .format(DEPTH_FORMAT)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .load_op(vk::AttachmentLoadOp::CLEAR)
                    .store_op(vk::AttachmentStoreOp::DONT_CARE)
                    .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                    .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                    .initial_layout(vk::ImageLayout::UNDEFINED)
                    .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL),
            ];
            let color_ref = [vk::AttachmentReference::default().attachment(0).layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
            let depth_ref = vk::AttachmentReference::default().attachment(1).layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
            let subpass = [vk::SubpassDescription::default()
                .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                .color_attachments(&color_ref)
                .depth_stencil_attachment(&depth_ref)];
            let render_pass = device.create_render_pass(&vk::RenderPassCreateInfo::default().attachments(&attachments).subpasses(&subpass), None)?;

            let bindings = [vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)];
            let set_layout = device.create_descriptor_set_layout(&vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings), None)?;
            let ranges = [vk::PushConstantRange::default()
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
                .size(std::mem::size_of::<Push>() as u32)];
            let set_layouts = [set_layout];
            let layout = device.create_pipeline_layout(&vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts).push_constant_ranges(&ranges), None)?;

            let module = |bytes: &[u8]| -> Result<vk::ShaderModule> {
                let words = ash::util::read_spv(&mut std::io::Cursor::new(bytes))?;
                Ok(device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&words), None)?)
            };
            let (vert, frag) = (module(VERT)?, module(FRAG)?);
            let stages = [
                vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::VERTEX).module(vert).name(c"main"),
                vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::FRAGMENT).module(frag).name(c"main"),
            ];
            let vbind = [vk::VertexInputBindingDescription::default().binding(0).stride(std::mem::size_of::<Vertex>() as u32).input_rate(vk::VertexInputRate::VERTEX)];
            let vattr = [
                vk::VertexInputAttributeDescription::default().location(0).binding(0).format(vk::Format::R32G32B32_SFLOAT).offset(0),
                vk::VertexInputAttributeDescription::default().location(1).binding(0).format(vk::Format::R32G32B32_SFLOAT).offset(12),
                vk::VertexInputAttributeDescription::default().location(2).binding(0).format(vk::Format::R32G32_SFLOAT).offset(24),
            ];
            let vertex_input = vk::PipelineVertexInputStateCreateInfo::default().vertex_binding_descriptions(&vbind).vertex_attribute_descriptions(&vattr);
            let assembly = vk::PipelineInputAssemblyStateCreateInfo::default().topology(vk::PrimitiveTopology::TRIANGLE_LIST);
            let viewport = vk::PipelineViewportStateCreateInfo::default().viewport_count(1).scissor_count(1);
            // Models wind their outside counter-clockwise; the projection's Y
            // flip keeps them that way in Vulkan's framebuffer.
            let raster = vk::PipelineRasterizationStateCreateInfo::default()
                .polygon_mode(vk::PolygonMode::FILL)
                .cull_mode(vk::CullModeFlags::BACK)
                .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
                .line_width(1.0);
            let multisample = vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(vk::SampleCountFlags::TYPE_1);
            let depth = vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(true)
                .depth_write_enable(true)
                .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL);
            let blend_att = [vk::PipelineColorBlendAttachmentState::default()
                .blend_enable(true)
                .src_color_blend_factor(vk::BlendFactor::ONE)
                .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ONE)
                .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .alpha_blend_op(vk::BlendOp::ADD)
                .color_write_mask(vk::ColorComponentFlags::RGBA)];
            let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_att);
            let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
            let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
            let info = [vk::GraphicsPipelineCreateInfo::default()
                .stages(&stages)
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&assembly)
                .viewport_state(&viewport)
                .rasterization_state(&raster)
                .multisample_state(&multisample)
                .depth_stencil_state(&depth)
                .color_blend_state(&blend)
                .dynamic_state(&dynamic)
                .layout(layout)
                .render_pass(render_pass)
                .subpass(0)];
            let pipeline = device
                .create_graphics_pipelines(vk::PipelineCache::null(), &info, None)
                .map_err(|(_, e)| anyhow!("scene pipeline: {e}"))?[0];
            device.destroy_shader_module(vert, None);
            device.destroy_shader_module(frag, None);

            let sampler = device.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::LINEAR)
                    .min_filter(vk::Filter::LINEAR)
                    .address_mode_u(vk::SamplerAddressMode::REPEAT)
                    .address_mode_v(vk::SamplerAddressMode::REPEAT)
                    .address_mode_w(vk::SamplerAddressMode::REPEAT),
                None,
            )?;
            let sizes = [vk::DescriptorPoolSize::default().ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).descriptor_count(256)];
            let pool = device.create_descriptor_pool(&vk::DescriptorPoolCreateInfo::default().max_sets(256).pool_sizes(&sizes), None)?;
            Ok(Self { device, allocator, render_pass, set_layout, layout, pipeline, sampler, pool, white: None })
        }
    }

    pub fn mesh(&self, vertices: &[Vertex], indices: &[u32]) -> Result<Mesh> {
        let upload = |bytes: &[u8], usage: vk::BufferUsageFlags, name: &str| -> Result<(vk::Buffer, Allocation)> {
            unsafe {
                let buffer = self
                    .device
                    .create_buffer(&vk::BufferCreateInfo::default().size(bytes.len().max(4) as u64).usage(usage).sharing_mode(vk::SharingMode::EXCLUSIVE), None)?;
                let reqs = self.device.get_buffer_memory_requirements(buffer);
                let mut a = alloc(&self.allocator, name, reqs, MemoryLocation::CpuToGpu, true)?;
                self.device.bind_buffer_memory(buffer, a.memory(), a.offset())?;
                a.mapped_slice_mut().ok_or_else(|| anyhow!("{name} not mapped"))?[..bytes.len()].copy_from_slice(bytes);
                Ok((buffer, a))
            }
        };
        let vbytes = unsafe { std::slice::from_raw_parts(vertices.as_ptr().cast::<u8>(), std::mem::size_of_val(vertices)) };
        let ibytes = unsafe { std::slice::from_raw_parts(indices.as_ptr().cast::<u8>(), std::mem::size_of_val(indices)) };
        let (vbuf, va) = upload(vbytes, vk::BufferUsageFlags::VERTEX_BUFFER, "scene-vertices")?;
        let (ibuf, ia) = upload(ibytes, vk::BufferUsageFlags::INDEX_BUFFER, "scene-indices")?;
        Ok(Mesh { vbuf, ibuf, count: indices.len() as u32, _allocs: [va, ia] })
    }

    /// Upload an sRGB texture (waits for the copy).
    pub fn texture(&self, img: &image::RgbaImage, cmd: vk::CommandBuffer, queue: vk::Queue, fence: vk::Fence) -> Result<Texture> {
        let (w, h) = img.dimensions();
        let d = &self.device;
        unsafe {
            let image = d.create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(vk::Format::R8G8B8A8_SRGB)
                    .extent(vk::Extent3D { width: w, height: h, depth: 1 })
                    .mip_levels(1)
                    .array_layers(1)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
                    .initial_layout(vk::ImageLayout::UNDEFINED),
                None,
            )?;
            let reqs = d.get_image_memory_requirements(image);
            let alloc_img = alloc(&self.allocator, "scene-texture", reqs, MemoryLocation::GpuOnly, false)?;
            d.bind_image_memory(image, alloc_img.memory(), alloc_img.offset())?;

            let bytes = img.as_raw();
            let staging = d.create_buffer(&vk::BufferCreateInfo::default().size(bytes.len() as u64).usage(vk::BufferUsageFlags::TRANSFER_SRC), None)?;
            let sreqs = d.get_buffer_memory_requirements(staging);
            let mut salloc = alloc(&self.allocator, "scene-texture-staging", sreqs, MemoryLocation::CpuToGpu, true)?;
            d.bind_buffer_memory(staging, salloc.memory(), salloc.offset())?;
            salloc.mapped_slice_mut().ok_or_else(|| anyhow!("staging not mapped"))?[..bytes.len()].copy_from_slice(bytes);

            d.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
            d.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT))?;
            crate::desktop::dmabuf::cmd_transition(
                d,
                cmd,
                image,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::AccessFlags::empty(),
                vk::AccessFlags::TRANSFER_WRITE,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
            );
            let region = vk::BufferImageCopy::default()
                .image_subresource(vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: 0, layer_count: 1 })
                .image_extent(vk::Extent3D { width: w, height: h, depth: 1 });
            d.cmd_copy_buffer_to_image(cmd, staging, image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &[region]);
            crate::desktop::dmabuf::cmd_transition(
                d,
                cmd,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::SHADER_READ,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
            );
            d.end_command_buffer(cmd)?;
            let cmds = [cmd];
            d.queue_submit(queue, &[vk::SubmitInfo::default().command_buffers(&cmds)], fence)?;
            d.wait_for_fences(&[fence], true, u64::MAX)?;
            d.reset_fences(&[fence])?;
            if let Ok(mut a) = self.allocator.lock() {
                let _ = a.free(salloc);
            }
            d.destroy_buffer(staging, None);

            let view = d.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(vk::Format::R8G8B8A8_SRGB)
                    .subresource_range(crate::desktop::dmabuf::color_range()),
                None,
            )?;
            let layouts = [self.set_layout];
            let set = d.allocate_descriptor_sets(&vk::DescriptorSetAllocateInfo::default().descriptor_pool(self.pool).set_layouts(&layouts))?[0];
            let info = [vk::DescriptorImageInfo::default().sampler(self.sampler).image_view(view).image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let write = [vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&info)];
            d.update_descriptor_sets(&write, &[]);
            Ok(Texture { _image: image, _view: view, set, _alloc: alloc_img })
        }
    }

    /// Bound for draws without a texture (tinted parts and the grid).
    pub fn ensure_white(&mut self, cmd: vk::CommandBuffer, queue: vk::Queue, fence: vk::Fence) -> Result<()> {
        if self.white.is_none() {
            let img = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
            self.white = Some(self.texture(&img, cmd, queue, fence)?);
        }
        Ok(())
    }

    /// Record one eye's pass into `cmd` (already begun): clear, then draw.
    pub fn record(&self, cmd: vk::CommandBuffer, framebuffer: vk::Framebuffer, extent: vk::Extent2D, view: &Mat4, proj: &Mat4, draws: &[Draw]) {
        let d = &self.device;
        let clear = [
            vk::ClearValue { color: vk::ClearColorValue { float32: [0.0; 4] } },
            vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 } },
        ];
        let area = vk::Rect2D { offset: vk::Offset2D::default(), extent };
        unsafe {
            d.cmd_begin_render_pass(
                cmd,
                &vk::RenderPassBeginInfo::default().render_pass(self.render_pass).framebuffer(framebuffer).render_area(area).clear_values(&clear),
                vk::SubpassContents::INLINE,
            );
            d.cmd_set_viewport(cmd, 0, &[vk::Viewport { x: 0.0, y: 0.0, width: extent.width as f32, height: extent.height as f32, min_depth: 0.0, max_depth: 1.0 }]);
            d.cmd_set_scissor(cmd, 0, &[area]);
            d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            let view_proj = math::mul(proj, view);
            for draw in draws {
                let Some(tex) = draw.texture.or(self.white.as_ref()) else { continue };
                let rot = math::rotation_rows(&math::mul(view, &draw.model));
                let push = Push {
                    mvp: math::mul(&view_proj, &draw.model),
                    n0: [rot[0][0], rot[0][1], rot[0][2], draw.tint[0]],
                    n1: [rot[1][0], rot[1][1], rot[1][2], draw.tint[1]],
                    n2: [rot[2][0], rot[2][1], rot[2][2], draw.tint[2]],
                    params: [draw.alpha, draw.mode as u32 as f32, draw.center[0], draw.center[1]],
                };
                let bytes = std::slice::from_raw_parts((&push as *const Push).cast::<u8>(), std::mem::size_of::<Push>());
                d.cmd_bind_descriptor_sets(cmd, vk::PipelineBindPoint::GRAPHICS, self.layout, 0, &[tex.set], &[]);
                d.cmd_push_constants(cmd, self.layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, bytes);
                d.cmd_bind_vertex_buffers(cmd, 0, &[draw.mesh.vbuf], &[0]);
                d.cmd_bind_index_buffer(cmd, draw.mesh.ibuf, 0, vk::IndexType::UINT32);
                d.cmd_draw_indexed(cmd, draw.mesh.count, 1, 0, 0, 0);
            }
            d.cmd_end_render_pass(cmd);
        }
    }

    /// Framebuffers over `images` (one per array layer each), with a depth
    /// buffer per layer.
    pub fn target(&self, images: &[vk::Image], format: vk::Format, extent: vk::Extent2D, layers: u32) -> Result<Target> {
        let d = &self.device;
        let mut depth = Vec::new();
        let mut depth_views = Vec::new();
        for _ in 0..layers {
            unsafe {
                let image = d.create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .format(DEPTH_FORMAT)
                        .extent(vk::Extent3D { width: extent.width, height: extent.height, depth: 1 })
                        .mip_levels(1)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
                        .initial_layout(vk::ImageLayout::UNDEFINED),
                    None,
                )?;
                let reqs = d.get_image_memory_requirements(image);
                let a = alloc(&self.allocator, "scene-depth", reqs, MemoryLocation::GpuOnly, false)?;
                d.bind_image_memory(image, a.memory(), a.offset())?;
                let view = d.create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(DEPTH_FORMAT)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::DEPTH,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        }),
                    None,
                )?;
                depth.push((image, a));
                depth_views.push(view);
            }
        }
        let mut framebuffers = Vec::new();
        let mut views = Vec::new();
        for &image in images {
            let mut per_layer = Vec::new();
            for layer in 0..layers {
                unsafe {
                    let view = d.create_image_view(
                        &vk::ImageViewCreateInfo::default()
                            .image(image)
                            .view_type(vk::ImageViewType::TYPE_2D)
                            .format(format)
                            .subresource_range(vk::ImageSubresourceRange {
                                aspect_mask: vk::ImageAspectFlags::COLOR,
                                base_mip_level: 0,
                                level_count: 1,
                                base_array_layer: layer,
                                layer_count: 1,
                            }),
                        None,
                    )?;
                    let attachments = [view, depth_views[layer as usize]];
                    let fb = d.create_framebuffer(
                        &vk::FramebufferCreateInfo::default()
                            .render_pass(self.render_pass)
                            .attachments(&attachments)
                            .width(extent.width)
                            .height(extent.height)
                            .layers(1),
                        None,
                    )?;
                    views.push(view);
                    per_layer.push(fb);
                }
            }
            framebuffers.push(per_layer);
        }
        Ok(Target { framebuffers, extent, _views: views, _depth: depth, _depth_views: depth_views })
    }
}

pub struct Target {
    /// `[image][layer]`.
    pub framebuffers: Vec<Vec<vk::Framebuffer>>,
    pub extent: vk::Extent2D,
    _views: Vec<vk::ImageView>,
    _depth: Vec<(vk::Image, Allocation)>,
    _depth_views: Vec<vk::ImageView>,
}
