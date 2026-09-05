//! Zero-copy path for captured desktop frames: import a PipeWire DMA-BUF as a
//! `VkImage` (via `VK_EXT_external_memory_dma_buf` + explicit DRM format
//! modifiers) so it can be blitted straight into an OpenXR swapchain image.
//! Also holds the SHM fallback (CPU frame → staging buffer → copy).
//!
//! Colour: desktop frames are sRGB-encoded 8-bit. When the swapchain is an
//! `_SRGB` format we import the DMA-BUF with the matching `_SRGB` view format
//! too, so a blit decodes → filters → re-encodes and the bytes come out
//! unchanged (and correctly filtered when scaled). With a UNORM swapchain we
//! import as UNORM — a straight byte copy either way.
use std::ffi::CStr;
use std::os::fd::{AsRawFd, IntoRawFd};

use anyhow::{anyhow, bail, Result};
use ash::vk;
use drm_fourcc::DrmFourcc;

use super::pw::{DmabufFrame, ShmFrame};

/// Device extensions the import path needs (all present on RADV/ANV/NVK).
pub const REQUIRED_EXTENSIONS: [&CStr; 4] = [
    ash::khr::external_memory_fd::NAME,
    ash::ext::external_memory_dma_buf::NAME,
    ash::ext::image_drm_format_modifier::NAME,
    ash::ext::queue_family_foreign::NAME,
];

/// Which of [`REQUIRED_EXTENSIONS`] the physical device offers. Returns the
/// list to enable (only when *all* are present — partial support is useless)
/// and whether that's the full set.
pub fn available_extensions(
    instance: &ash::Instance,
    phys: vk::PhysicalDevice,
) -> (Vec<*const std::os::raw::c_char>, bool) {
    let props = match unsafe { instance.enumerate_device_extension_properties(phys) } {
        Ok(p) => p,
        Err(e) => {
            log::warn!("desktop: enumerate_device_extension_properties failed: {e}");
            return (Vec::new(), false);
        }
    };
    let has = |name: &CStr| {
        props.iter().any(|p| p.extension_name_as_c_str().map_or(false, |n| n == name))
    };
    let all = REQUIRED_EXTENSIONS.iter().all(|n| has(n));
    if !all {
        let missing: Vec<String> = REQUIRED_EXTENSIONS
            .iter()
            .filter(|n| !has(n))
            .map(|n| n.to_string_lossy().into_owned())
            .collect();
        log::warn!("desktop: DMA-BUF import unavailable, missing {missing:?} (SHM fallback only)");
        return (Vec::new(), false);
    }
    (REQUIRED_EXTENSIONS.iter().map(|n| n.as_ptr()).collect(), true)
}

/// A (fourcc, modifier) pair we can import — advertised to PipeWire so the
/// compositor allocates frames we can map without a copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrmFormat {
    pub fourcc: DrmFourcc,
    pub modifier: u64,
}

/// What the GPU can do for us, decided once at startup.
#[derive(Clone, Debug)]
pub struct Caps {
    /// All four import extensions enabled on the device.
    pub dmabuf: bool,
    /// Formats/modifiers we can import (empty when `dmabuf` is false).
    pub formats: Vec<DrmFormat>,
    /// The swapchain format the screens render into (same as the UI panels).
    pub swap_format: vk::Format,
    /// Runtime supports XR_KHR_composition_layer_cylinder (curved screens).
    pub curved: bool,
    /// Runtime supports XR_KHR_composition_layer_color_scale_bias (opacity).
    pub color_scale: bool,
    /// Screencast frame-rate cap negotiated with PipeWire (0 = none).
    pub max_fps: u32,
    /// Mirrored screens are downscaled to this height (0 = native).
    pub max_height: u32,
}

impl Caps {
    pub fn query(
        instance: &ash::Instance,
        phys: vk::PhysicalDevice,
        dmabuf: bool,
        swap_format: vk::Format,
    ) -> Self {
        let mut formats = Vec::new();
        if dmabuf {
            let srgb = is_srgb(swap_format);
            for fourcc in [DrmFourcc::Argb8888, DrmFourcc::Xrgb8888, DrmFourcc::Abgr8888, DrmFourcc::Xbgr8888] {
                let Some(vkf) = fourcc_to_vk(fourcc, srgb) else { continue };
                for m in importable_modifiers(instance, phys, vkf) {
                    formats.push(DrmFormat { fourcc, modifier: m });
                }
            }
            log::info!("desktop: {} importable DRM format/modifier pairs", formats.len());
            for f in &formats {
                log::debug!("  {} 0x{:016x}", f.fourcc, f.modifier);
            }
        }
        Self {
            dmabuf: dmabuf && !formats.is_empty(),
            formats,
            swap_format,
            curved: false,
            color_scale: false,
            max_fps: 0,
            max_height: 0,
        }
    }
}

pub fn is_srgb(f: vk::Format) -> bool {
    matches!(f, vk::Format::B8G8R8A8_SRGB | vk::Format::R8G8B8A8_SRGB)
}

/// Memory byte order of a fourcc: true when bytes are R,G,B,A (Abgr8888 /
/// Xbgr8888 — "ABGR" little-endian packed), false for B,G,R,A (Argb8888).
pub fn fourcc_is_rgba_order(f: DrmFourcc) -> bool {
    matches!(f, DrmFourcc::Abgr8888 | DrmFourcc::Xbgr8888)
}

pub fn vk_is_rgba_order(f: vk::Format) -> bool {
    matches!(f, vk::Format::R8G8B8A8_SRGB | vk::Format::R8G8B8A8_UNORM)
}

pub fn fourcc_to_vk(f: DrmFourcc, srgb: bool) -> Option<vk::Format> {
    Some(match (f, srgb) {
        (DrmFourcc::Argb8888 | DrmFourcc::Xrgb8888, true) => vk::Format::B8G8R8A8_SRGB,
        (DrmFourcc::Argb8888 | DrmFourcc::Xrgb8888, false) => vk::Format::B8G8R8A8_UNORM,
        (DrmFourcc::Abgr8888 | DrmFourcc::Xbgr8888, true) => vk::Format::R8G8B8A8_SRGB,
        (DrmFourcc::Abgr8888 | DrmFourcc::Xbgr8888, false) => vk::Format::R8G8B8A8_UNORM,
        _ => return None,
    })
}

/// Single-plane modifiers the device can import a DMA-BUF of `format` with,
/// for sampling/transfer-src use.
fn importable_modifiers(instance: &ash::Instance, phys: vk::PhysicalDevice, format: vk::Format) -> Vec<u64> {
    // Two-call pattern: count, then fill.
    let mut list = vk::DrmFormatModifierPropertiesListEXT::default();
    let mut props = vk::FormatProperties2::default().push_next(&mut list);
    unsafe { instance.get_physical_device_format_properties2(phys, format, &mut props) };
    let count = list.drm_format_modifier_count as usize;
    if count == 0 {
        return Vec::new();
    }
    let mut mods = vec![vk::DrmFormatModifierPropertiesEXT::default(); count];
    let mut list = vk::DrmFormatModifierPropertiesListEXT::default().drm_format_modifier_properties(&mut mods);
    let mut props = vk::FormatProperties2::default().push_next(&mut list);
    unsafe { instance.get_physical_device_format_properties2(phys, format, &mut props) };
    let filled = list.drm_format_modifier_count as usize;
    mods.truncate(filled);

    let mut out = Vec::new();
    for m in mods {
        if m.drm_format_modifier_plane_count != 1 {
            continue;
        }
        if !m.drm_format_modifier_tiling_features.contains(vk::FormatFeatureFlags::TRANSFER_SRC) {
            continue;
        }
        // Confirm the modifier is importable as a DMA-BUF for a 2D transfer-src image.
        let mut mod_info = vk::PhysicalDeviceImageDrmFormatModifierInfoEXT::default()
            .drm_format_modifier(m.drm_format_modifier)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let mut ext_info = vk::PhysicalDeviceExternalImageFormatInfo::default()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let info = vk::PhysicalDeviceImageFormatInfo2::default()
            .format(format)
            .ty(vk::ImageType::TYPE_2D)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(vk::ImageUsageFlags::TRANSFER_SRC)
            .push_next(&mut mod_info)
            .push_next(&mut ext_info);
        let mut ext_props = vk::ExternalImageFormatProperties::default();
        let mut out_props = vk::ImageFormatProperties2::default().push_next(&mut ext_props);
        let ok = unsafe { instance.get_physical_device_image_format_properties2(phys, &info, &mut out_props) }.is_ok();
        if ok
            && ext_props
                .external_memory_properties
                .external_memory_features
                .contains(vk::ExternalMemoryFeatureFlags::IMPORTABLE)
        {
            out.push(m.drm_format_modifier);
        }
    }
    out
}

/// An imported DMA-BUF frame living on the GPU. Destroy with [`Importer::destroy`]
/// once the blit that reads it has completed.
pub struct Imported {
    pub image: vk::Image,
    pub memory: vk::DeviceMemory,
    pub extent: vk::Extent2D,
}

pub struct Importer {
    device: ash::Device,
    ext_fd: ash::khr::external_memory_fd::Device,
    queue_family: u32,
    srgb: bool,
}

impl Importer {
    pub fn new(instance: &ash::Instance, device: &ash::Device, queue_family: u32, swap_format: vk::Format) -> Self {
        Self {
            device: device.clone(),
            ext_fd: ash::khr::external_memory_fd::Device::new(instance, device),
            queue_family,
            srgb: is_srgb(swap_format),
        }
    }

    /// Import plane 0 of `frame` (the only plane for the formats we advertise).
    /// The caller keeps ownership of the frame's fd; we dup it and hand the dup
    /// to Vulkan (which owns it from then on).
    pub fn import(&self, frame: &DmabufFrame) -> Result<Imported> {
        let plane = frame.planes.first().ok_or_else(|| anyhow!("DMA-BUF frame has no planes"))?;
        let format = fourcc_to_vk(frame.format.fourcc, self.srgb)
            .ok_or_else(|| anyhow!("unsupported fourcc {}", frame.format.fourcc))?;
        let extent = vk::Extent2D { width: frame.format.width, height: frame.format.height };

        let layouts = [vk::SubresourceLayout {
            offset: plane.offset as u64,
            size: 0,
            row_pitch: plane.stride as u64,
            array_pitch: 0,
            depth_pitch: 0,
        }];
        let mut modifier_info = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
            .drm_format_modifier(frame.format.modifier)
            .plane_layouts(&layouts);
        let mut external_info = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let create = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D { width: extent.width, height: extent.height, depth: 1 })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(vk::ImageUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut modifier_info)
            .push_next(&mut external_info);
        let image = unsafe { self.device.create_image(&create, None) }?;

        // Memory: what the fd allows ∩ what the image needs; dedicated import.
        let dup = match plane.fd.try_clone() {
            Ok(d) => d,
            Err(e) => {
                unsafe { self.device.destroy_image(image, None) };
                bail!("dup DMA-BUF fd: {e}");
            }
        };
        let mut fd_props = vk::MemoryFdPropertiesKHR::default();
        if let Err(e) = unsafe {
            self.ext_fd.get_memory_fd_properties(
                vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
                dup.as_raw_fd(),
                &mut fd_props,
            )
        } {
            unsafe { self.device.destroy_image(image, None) };
            bail!("vkGetMemoryFdPropertiesKHR: {e}");
        }
        let mut dedicated_reqs = vk::MemoryDedicatedRequirements::default();
        let mut reqs = vk::MemoryRequirements2::default().push_next(&mut dedicated_reqs);
        unsafe {
            self.device
                .get_image_memory_requirements2(&vk::ImageMemoryRequirementsInfo2::default().image(image), &mut reqs)
        };
        let bits = reqs.memory_requirements.memory_type_bits & fd_props.memory_type_bits;
        let Some(type_index) = (0..32).find(|i| bits & (1 << i) != 0) else {
            unsafe { self.device.destroy_image(image, None) };
            bail!("no memory type can import this DMA-BUF");
        };
        let raw_fd = dup.into_raw_fd(); // Vulkan owns it on success
        let mut import_info = vk::ImportMemoryFdInfoKHR::default()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
            .fd(raw_fd);
        let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image);
        let alloc = vk::MemoryAllocateInfo::default()
            .allocation_size(reqs.memory_requirements.size)
            .memory_type_index(type_index)
            .push_next(&mut import_info)
            .push_next(&mut dedicated);
        let memory = match unsafe { self.device.allocate_memory(&alloc, None) } {
            Ok(m) => m,
            Err(e) => {
                unsafe {
                    libc::close(raw_fd);
                    self.device.destroy_image(image, None);
                }
                bail!("import DMA-BUF memory: {e}");
            }
        };
        if let Err(e) = unsafe { self.device.bind_image_memory(image, memory, 0) } {
            unsafe {
                self.device.free_memory(memory, None);
                self.device.destroy_image(image, None);
            }
            bail!("bind imported memory: {e}");
        }
        Ok(Imported { image, memory, extent })
    }

    /// Record: acquire `src` from the foreign (compositor) queue, blit it over
    /// `dst` (assumed already TRANSFER_DST_OPTIMAL), release `src` back.
    pub fn cmd_blit(&self, cmd: vk::CommandBuffer, src: &Imported, dst: vk::Image, dst_extent: vk::Extent2D) {
        let range = color_range();
        unsafe {
            let acquire = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT)
                .dst_queue_family_index(self.queue_family)
                .image(src.image)
                .subresource_range(range);
            self.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[acquire],
            );
            let sub = vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            };
            let blit = vk::ImageBlit {
                src_subresource: sub,
                src_offsets: [
                    vk::Offset3D { x: 0, y: 0, z: 0 },
                    vk::Offset3D { x: src.extent.width as i32, y: src.extent.height as i32, z: 1 },
                ],
                dst_subresource: sub,
                dst_offsets: [
                    vk::Offset3D { x: 0, y: 0, z: 0 },
                    vk::Offset3D { x: dst_extent.width as i32, y: dst_extent.height as i32, z: 1 },
                ],
            };
            self.device.cmd_blit_image(
                cmd,
                src.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                dst,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[blit],
                vk::Filter::LINEAR,
            );
            let release = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                .dst_access_mask(vk::AccessFlags::empty())
                .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .src_queue_family_index(self.queue_family)
                .dst_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT)
                .image(src.image)
                .subresource_range(range);
            self.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[release],
            );
        }
    }

    pub fn destroy(&self, img: Imported) {
        unsafe {
            self.device.destroy_image(img.image, None);
            self.device.free_memory(img.memory, None);
        }
    }
}

pub fn color_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    }
}

/// Transition helper for the destination swapchain image.
pub fn cmd_transition(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    old: vk::ImageLayout,
    new: vk::ImageLayout,
    src_access: vk::AccessFlags,
    dst_access: vk::AccessFlags,
    src_stage: vk::PipelineStageFlags,
    dst_stage: vk::PipelineStageFlags,
) {
    let barrier = vk::ImageMemoryBarrier::default()
        .src_access_mask(src_access)
        .dst_access_mask(dst_access)
        .old_layout(old)
        .new_layout(new)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(color_range());
    unsafe {
        device.cmd_pipeline_barrier(cmd, src_stage, dst_stage, vk::DependencyFlags::empty(), &[], &[], &[barrier]);
    }
}

// --- SHM fallback --------------------------------------------------------------

/// Host-visible staging buffer reused across frames (grown on demand).
pub struct Staging {
    pub buffer: vk::Buffer,
    pub allocation: Option<gpu_allocator::vulkan::Allocation>,
    pub size: u64,
}

impl Staging {
    pub fn ensure(
        this: &mut Option<Staging>,
        device: &ash::Device,
        allocator: &std::sync::Mutex<gpu_allocator::vulkan::Allocator>,
        size: u64,
    ) -> Result<()> {
        if this.as_ref().is_some_and(|s| s.size >= size) {
            return Ok(());
        }
        if let Some(old) = this.take() {
            old.destroy(device, allocator);
        }
        let buffer = unsafe {
            device.create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(size)
                    .usage(vk::BufferUsageFlags::TRANSFER_SRC)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )?
        };
        let reqs = unsafe { device.get_buffer_memory_requirements(buffer) };
        let allocation = allocator
            .lock()
            .map_err(|_| anyhow!("allocator poisoned"))?
            .allocate(&gpu_allocator::vulkan::AllocationCreateDesc {
                name: "desktop-shm-staging",
                requirements: reqs,
                location: gpu_allocator::MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| anyhow!("staging alloc: {e}"))?;
        unsafe { device.bind_buffer_memory(buffer, allocation.memory(), allocation.offset())? };
        *this = Some(Staging { buffer, allocation: Some(allocation), size });
        Ok(())
    }

    /// Copy a CPU frame into the staging buffer, swizzling to the swapchain's
    /// channel order if needed, and record the buffer→image copy. `dst` must be
    /// TRANSFER_DST_OPTIMAL and the same size as the frame.
    pub fn cmd_upload(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: &ShmFrame,
        swap_format: vk::Format,
        dst: vk::Image,
    ) -> Result<()> {
        let w = frame.format.width as usize;
        let h = frame.format.height as usize;
        let need = (w * h * 4) as u64;
        if need > self.size {
            bail!("staging too small");
        }
        let swizzle = fourcc_is_rgba_order(frame.format.fourcc) != vk_is_rgba_order(swap_format);
        let alloc = self.allocation.as_mut().ok_or_else(|| anyhow!("staging unmapped"))?;
        let dstp = alloc.mapped_slice_mut().ok_or_else(|| anyhow!("staging not host-visible"))?;
        let stride = frame.stride as usize;
        for y in 0..h {
            let src_row = &frame.data[y * stride..y * stride + w * 4];
            let dst_row = &mut dstp[y * w * 4..(y + 1) * w * 4];
            if swizzle {
                for (d, s) in dst_row.chunks_exact_mut(4).zip(src_row.chunks_exact(4)) {
                    d[0] = s[2];
                    d[1] = s[1];
                    d[2] = s[0];
                    d[3] = s[3];
                }
            } else {
                dst_row.copy_from_slice(src_row);
            }
        }
        let region = vk::BufferImageCopy::default()
            .buffer_offset(0)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_extent(vk::Extent3D { width: w as u32, height: h as u32, depth: 1 });
        unsafe {
            device.cmd_copy_buffer_to_image(cmd, self.buffer, dst, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &[region]);
        }
        Ok(())
    }

    pub fn destroy(mut self, device: &ash::Device, allocator: &std::sync::Mutex<gpu_allocator::vulkan::Allocator>) {
        if let Some(a) = self.allocation.take() {
            if let Ok(mut al) = allocator.lock() {
                let _ = al.free(a);
            }
        }
        unsafe { device.destroy_buffer(self.buffer, None) };
    }
}
