use ash::vk;
use std::ffi::CStr;
use std::sync::Arc;
use std::sync::RwLock;

use super::device::DeviceHandles;
use super::gpu_ptr::GpuPtr;
use super::gpu_resources::GpuResources;
use super::gpu_resources::ImageData;
use super::semaphore::Semaphore;
use crate::*;

pub(crate) enum PipelineType {
    Graphics,
    Compute,
    //RayTracing,
    //Mesh,
}

impl PipelineType {
    fn bind_point(&self) -> vk::PipelineBindPoint {
        match self {
            PipelineType::Graphics => vk::PipelineBindPoint::GRAPHICS,
            PipelineType::Compute => vk::PipelineBindPoint::COMPUTE,
            //PipelineType::RayTracing => vk::PipelineBindPoint::RAY_TRACING_KHR,
            //PipelineType::Mesh => vk::PipelineBindPoint::GRAPHICS,
        }
    }
}

pub struct Pipeline {
    pub(super) device: Arc<DeviceHandles>,
    pub(super) inner: vk::Pipeline,
    pub(super) ty: PipelineType,
}

impl Pipeline {
    pub fn set_object_name(&self, name: &CStr) {
        self.device.set_object_name(self.inner, name);
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.inner.destroy_pipeline(self.inner, None);
        }
    }
}

pub(super) struct SemaphoreInfo {
    pub(super) semaphore: vk::Semaphore,
    pub(super) value: u64,
    pub(super) stage: Stage,
}

pub(super) struct RenderPassState {
    depth_test: bool,
    stencil_test: bool,
}

pub struct CommandBuffer {
    // Handles
    pub(super) device: Arc<DeviceHandles>,
    pub(super) heap: Arc<RwLock<GpuResources>>,
    pub(super) inner: vk::CommandBuffer,
    pub(super) command_pool_idx: u32,
    // Subpass state
    pub(super) wait: Vec<SemaphoreInfo>,
    pub(super) signal: Vec<SemaphoreInfo>,
    pub(super) layout_transition_queue: Vec<LayoutTransition>,
    // Presentation state
    pub(super) presentation: Option<GpuImage>,
    // Render pass state
    pub(super) render_pass_state: Option<RenderPassState>,
}

// Common helpers
impl CommandBuffer {
    fn push_data(&mut self, data: PushData) {
        if data.is_empty() {
            return;
        }
        unsafe {
            match (
                &self.device.descriptor_heap,
                &self.device.descriptor_indexing,
            ) {
                (None, None) => unreachable!(
                    "If descriptor heaps are not enabled, descriptor indexing data should exist"
                ),
                (None, Some(descriptor_indexing_data)) => {
                    let stage_flags = vk::ShaderStageFlags::VERTEX
                        | vk::ShaderStageFlags::FRAGMENT
                        | vk::ShaderStageFlags::COMPUTE;

                    self.device.inner.cmd_push_constants(
                        self.inner,
                        descriptor_indexing_data.pipeline_layout,
                        stage_flags,
                        0,
                        data,
                    );
                }
                (Some(descriptor_heap), _) => {
                    let props = self
                        .device
                        .descriptor_heap_props
                        .as_ref()
                        .expect("Descriptor heap feature is not enabled.");

                    assert!(
                        props.max_push_data_size >= data.len() as u64,
                        "Push data too large"
                    );
                    // Calling cmd_push_data with a zero length is invalid
                    let push_data_info = vk::PushDataInfoEXT::default()
                        .data(vk::HostAddressRangeConstEXT::default().address(data));

                    descriptor_heap.cmd_push_data(self.inner, &push_data_info);
                }
            }
        }
    }

    pub(super) unsafe fn transition_image_layout(
        &self,
        image: vk::Image,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
        src_stage_mask: vk::PipelineStageFlags2,
        src_access_mask: vk::AccessFlags2,
        dst_stage_mask: vk::PipelineStageFlags2,
        dst_access_mask: vk::AccessFlags2,
    ) {
        if old_layout == new_layout {
            //return;
        }
        let aspect_flags = match new_layout {
            vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL => vk::ImageAspectFlags::DEPTH,
            vk::ImageLayout::STENCIL_ATTACHMENT_OPTIMAL => vk::ImageAspectFlags::STENCIL,
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL => {
                vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
            }
            _ => vk::ImageAspectFlags::COLOR,
        };

        let image_memory_barrier = [vk::ImageMemoryBarrier2::default()
            .src_stage_mask(src_stage_mask)
            .src_access_mask(src_access_mask)
            .dst_stage_mask(dst_stage_mask)
            .dst_access_mask(dst_access_mask)
            .old_layout(old_layout)
            .new_layout(new_layout)
            .image(image)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(aspect_flags)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1),
            )];

        let dependency_info =
            vk::DependencyInfo::default().image_memory_barriers(&image_memory_barrier);
        unsafe {
            self.device
                .inner
                .cmd_pipeline_barrier2(self.inner, &dependency_info);
        }
    }

    pub(super) unsafe fn multiple_layout_transition(&self, transitions: &[LayoutTransition]) {
        // TODO: array the transitions
        for transition in transitions {
            let LayoutTransition {
                image,
                new_layout,
                src_stage_mask,
                src_access_mask,
                dst_stage_mask,
                dst_access_mask,
            } = transition;

            let (image, old_layout) = {
                let guard = self.heap.read().unwrap();
                let image = guard.get_image(*image);
                let old_layout = image.current_layout.get();
                image.current_layout.set(*new_layout);
                (image.inner, old_layout)
            };

            unsafe {
                self.transition_image_layout(
                    image,
                    old_layout,
                    *new_layout,
                    *src_stage_mask,
                    *src_access_mask,
                    *dst_stage_mask,
                    *dst_access_mask,
                );
            }
        }
    }

    pub(super) fn flush_layout_transitions(&mut self) {
        unsafe {
            self.multiple_layout_transition(&self.layout_transition_queue);
            self.layout_transition_queue.clear();
        }
    }

    unsafe fn set_fixed_dynamic_states(&mut self, extent: vk::Extent2D) {
        unsafe {
            let viewports = [vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: extent.width as f32,
                height: extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }];

            self.device
                .inner
                .cmd_set_viewport(self.inner, 0, &viewports);

            let scissors = [vk::Rect2D {
                offset: vk::Offset2D::default(),
                extent,
            }];

            self.device.inner.cmd_set_scissor(self.inner, 0, &scissors);

            self.set_depth_stencil_state(None);

            self.set_blend_state(None);
        }
    }

    unsafe fn set_stencil_state(&self, flags: vk::StencilFaceFlags, info: &StencilInfo) {
        unsafe {
            self.device
                .inner
                .cmd_set_stencil_write_mask(self.inner, flags, info.write_mask);

            self.device
                .inner
                .cmd_set_stencil_compare_mask(self.inner, flags, info.read_mask);

            self.device
                .inner
                .cmd_set_stencil_reference(self.inner, flags, info.reference);

            self.device.inner.cmd_set_stencil_op(
                self.inner,
                flags,
                info.fail_op.into(),
                info.pass_op.into(),
                info.depth_fail_op.into(),
                info.compare_op.into(),
            );
        }
    }
}

impl CommandRHI for CommandBuffer {
    type GpuPtr = GpuPtr;
    type Pipeline = Pipeline;
    type Semaphore = Semaphore;

    fn copy_buffer(&mut self, src: Self::GpuPtr, dst: Self::GpuPtr) {
        let heap = self.heap.read().unwrap();
        let src_buffer = heap.ptr_to_buffer(src);
        let dst_buffer = heap.ptr_to_buffer(dst);

        let src_size = src_buffer.len();
        let dst_size = dst_buffer.len();

        let src_offset = src.offset;
        let dst_offset = dst.offset;

        assert!(dst_size >= src_size);
        let info = [vk::BufferCopy::default()
            .src_offset(src_offset as u64)
            .dst_offset(dst_offset as u64)
            .size(src_size)];

        unsafe {
            self.device.inner.cmd_copy_buffer(
                self.inner,
                src_buffer.inner,
                dst_buffer.inner,
                &info,
            );
        }
    }

    fn copy_to_image(&mut self, src: Self::GpuPtr, dst: GpuImageRegion) {
        assert!(
            dst.is_within_bounds(),
            "Provided offset and extent are out of bounds for the destination image."
        );

        let heap = self.heap.read().unwrap();
        let src_buffer = heap.ptr_to_buffer(src);
        let dst_image = heap.get_image(dst.image);

        let regions = [vk::BufferImageCopy::default()
            .buffer_offset(src.offset as u64)
            // Tightly packed
            .buffer_image_height(0)
            .buffer_row_length(0)
            .image_offset(dst.vk_offset())
            .image_extent(dst.vk_extent())
            .image_subresource(dst.vk_subresource())];

        unsafe {
            self.device.inner.cmd_copy_buffer_to_image(
                self.inner,
                src_buffer.inner,
                dst_image.inner,
                dst_image.current_layout.get(),
                &regions,
            );
        }
    }

    fn copy_from_image(&mut self, src: GpuImageRegion, dst: Self::GpuPtr) {
        assert!(
            src.is_within_bounds(),
            "Provided offset and extent are out of bounds for the source image."
        );

        let heap = self.heap.read().unwrap();
        let src_image = heap.get_image(src.image);
        let dst_buffer = heap.ptr_to_buffer(dst);

        let regions = [vk::BufferImageCopy::default()
            .buffer_offset(dst.offset as u64)
            // Tightly packed
            .buffer_image_height(0)
            .buffer_row_length(0)
            .image_offset(src.vk_offset())
            .image_extent(src.vk_extent())
            .image_subresource(src.vk_subresource())];

        unsafe {
            self.device.inner.cmd_copy_image_to_buffer(
                self.inner,
                src_image.inner,
                src_image.current_layout.get(),
                dst_buffer.inner,
                &regions,
            );
        }
    }

    fn copy_image(&mut self, src: GpuImageRegion, dst: GpuImageRegion) {
        assert!(
            src.is_within_bounds(),
            "Provided offset and extent are out of bounds for the source image."
        );
        assert!(
            dst.is_within_bounds(),
            "Provided offset and extent are out of bounds for the destination image."
        );

        let heap = self.heap.read().unwrap();
        let src_image = heap.get_image(src.image);
        let dst_image = heap.get_image(dst.image);

        // A plain copy is cheaper and is the only option for multisampled and integer images,
        // so only blit when scaling or format conversion is actually required.
        let needs_blit = src.extent != dst.extent || src_image.desc.format != dst_image.desc.format;

        unsafe {
            if needs_blit {
                let regions = [vk::ImageBlit::default()
                    .src_subresource(src.vk_subresource())
                    .src_offsets([src.vk_offset(), src.vk_end_offset()])
                    .dst_subresource(dst.vk_subresource())
                    .dst_offsets([dst.vk_offset(), dst.vk_end_offset()])];

                self.device.inner.cmd_blit_image(
                    self.inner,
                    src_image.inner,
                    src_image.current_layout.get(),
                    dst_image.inner,
                    dst_image.current_layout.get(),
                    &regions,
                    vk::Filter::LINEAR,
                );
            } else {
                let regions = [vk::ImageCopy::default()
                    .src_subresource(src.vk_subresource())
                    .src_offset(src.vk_offset())
                    .dst_subresource(dst.vk_subresource())
                    .dst_offset(dst.vk_offset())
                    .extent(src.vk_extent())];

                self.device.inner.cmd_copy_image(
                    self.inner,
                    src_image.inner,
                    src_image.current_layout.get(),
                    dst_image.inner,
                    dst_image.current_layout.get(),
                    &regions,
                );
            }
        }
    }

    fn bind_descriptor_heap(&mut self, resource_heap: Self::GpuPtr, sampler_heap: Self::GpuPtr) {
        let resource_heap_null = resource_heap.is_null();
        let sampler_heap_null = sampler_heap.is_null();

        let heap = self.heap.read().unwrap();

        let descriptor_heap = self
            .device
            .descriptor_heap
            .as_ref()
            .expect("Descriptor heap feature is not enabled.");
        let props = self
            .device
            .descriptor_heap_props
            .as_ref()
            .expect("Descriptor heap feature is not enabled.");

        if !resource_heap_null {
            let resource_heap_buf = heap.ptr_to_buffer(resource_heap);

            let resource_addr = unsafe {
                self.device.inner.get_buffer_device_address(
                    &vk::BufferDeviceAddressInfo::default().buffer(resource_heap_buf.inner),
                ) + resource_heap.offset as u64
            };

            let resource_heap_size = resource_heap.size as u64;

            let heap_bind_info = vk::BindHeapInfoEXT::default()
                .heap_range(
                    vk::DeviceAddressRangeKHR::default()
                        .address(resource_addr)
                        .size(resource_heap_size),
                )
                .reserved_range_offset(resource_heap_size - props.min_resource_heap_reserved_range)
                .reserved_range_size(props.min_resource_heap_reserved_range);

            unsafe {
                descriptor_heap.cmd_bind_resource_heap(self.inner, &heap_bind_info);
            }
        }
        if !sampler_heap_null {
            let sampler_heap_buf = heap.ptr_to_buffer(sampler_heap);

            let sampler_addr = unsafe {
                self.device.inner.get_buffer_device_address(
                    &vk::BufferDeviceAddressInfo::default().buffer(sampler_heap_buf.inner),
                ) + sampler_heap.offset as u64
            };

            let sampler_heap_size = sampler_heap.size as u64 / props.sampler_heap_alignment
                * props.sampler_heap_alignment;

            let sampler_heap_reserved_size = props.min_sampler_heap_reserved_range
                + (props.sampler_heap_alignment - 1) / props.sampler_heap_alignment
                    * props.sampler_heap_alignment;
            let sampler_heap_reserved_offset = sampler_heap_size - sampler_heap_reserved_size;

            let heap_bind_info = vk::BindHeapInfoEXT::default()
                .heap_range(
                    vk::DeviceAddressRangeKHR::default()
                        .address(sampler_addr)
                        .size(sampler_heap_size),
                )
                .reserved_range_offset(sampler_heap_reserved_offset)
                .reserved_range_size(sampler_heap_reserved_size);

            unsafe {
                descriptor_heap.cmd_bind_sampler_heap(self.inner, &heap_bind_info);
            }
        }

        if !sampler_heap_null || !resource_heap_null {
            let barriers = [vk::MemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::HOST)
                .src_access_mask(vk::AccessFlags2::HOST_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::ALL_GRAPHICS)
                .dst_access_mask(
                    vk::AccessFlags2::RESOURCE_HEAP_READ_EXT
                        | vk::AccessFlags2::SAMPLER_HEAP_READ_EXT,
                )];

            unsafe {
                self.device.inner.cmd_pipeline_barrier2(
                    self.inner,
                    &vk::DependencyInfo::default().memory_barriers(&barriers),
                );
            }
        }
    }

    fn barrier(&mut self, before: Stage, after: Stage, _hazards: Hazard) {
        unsafe {
            // TODO: use hazards
            // A read after write indicates a true dependency which is the strictest
            // synchronization option we have. Might be a good idea to make this more relaxed
            let barriers = [vk::MemoryBarrier2::default()
                .src_stage_mask(before.into())
                .src_access_mask(vk::AccessFlags2::MEMORY_WRITE)
                .dst_stage_mask(after.into())
                .dst_access_mask(vk::AccessFlags2::MEMORY_READ)];

            let dependency_info = vk::DependencyInfo::default().memory_barriers(&barriers);
            self.device
                .inner
                .cmd_pipeline_barrier2(self.inner, &dependency_info);
        }
    }

    fn image_barrier(&mut self, before: Stage, after: Stage, image: GpuImage, layout: ImageLayout) {
        use vk::AccessFlags2 as vkaf;
        // TODO: use a hazards flag
        let src_access_mask = match before {
            Stage::Host => vkaf::HOST_WRITE,
            Stage::Transfer | Stage::Copy | Stage::Blit => vkaf::TRANSFER_WRITE,
            Stage::ColorAttachmentOutput => vkaf::COLOR_ATTACHMENT_WRITE,
            _ => vkaf::empty(),
        };
        // The transition must also make the image visible to the consumer
        let dst_access_mask = match after {
            Stage::Transfer | Stage::Copy | Stage::Blit => {
                vkaf::TRANSFER_READ | vkaf::TRANSFER_WRITE
            }
            Stage::VertexShader | Stage::FragmentShader | Stage::ComputeShader => {
                vkaf::SHADER_SAMPLED_READ | vkaf::SHADER_STORAGE_READ
            }
            Stage::AllGraphics | Stage::AllCommands => vkaf::MEMORY_READ | vkaf::MEMORY_WRITE,
            _ => vkaf::empty(),
        };

        self.layout_transition_queue.push(LayoutTransition {
            image,
            new_layout: layout.into(),
            src_stage_mask: before.into(),
            src_access_mask: src_access_mask,
            dst_stage_mask: after.into(),
            dst_access_mask: dst_access_mask,
        });
        self.flush_layout_transitions();
    }

    fn signal_after(&mut self, stage: Stage, semaphore: &Self::Semaphore, value: u64) {
        self.signal.push(SemaphoreInfo {
            semaphore: semaphore.inner,
            value,
            stage,
        });
    }

    fn wait_before(&mut self, stage: Stage, semaphore: &Self::Semaphore, value: u64) {
        self.wait.push(SemaphoreInfo {
            semaphore: semaphore.inner,
            value,
            stage,
        });
    }

    fn set_pipeline(&mut self, pipeline: &Self::Pipeline) {
        let bind_point = pipeline.ty.bind_point();
        unsafe {
            self.device
                .inner
                .cmd_bind_pipeline(self.inner, bind_point, pipeline.inner);
        }
    }

    fn set_depth_stencil_state(&mut self, state: Option<&DepthStencilInfo>) {
        let Some(RenderPassState {
            depth_test,
            stencil_test,
        }) = self.render_pass_state
        else {
            return;
        };

        let depth = state.filter(|_| depth_test);
        let stencil = state.filter(|_| stencil_test);

        unsafe {
            let device = &self.device.inner;

            device.cmd_set_depth_test_enable(self.inner, depth.is_some_and(|s| s.depth_read));
            device.cmd_set_depth_write_enable(self.inner, depth.is_some_and(|s| s.depth_write));
            device.cmd_set_depth_bias_enable(self.inner, depth.is_some());
            device.cmd_set_stencil_test_enable(self.inner, stencil.is_some());

            if let Some(state) = depth {
                device.cmd_set_depth_bias(
                    self.inner,
                    state.depth_bias,
                    state.depth_bias_clamp,
                    state.depth_bias_slope_factor,
                );
                device.cmd_set_depth_compare_op(self.inner, state.depth_test.into());
            }

            if let Some(state) = stencil {
                if state.stencil_back == state.stencil_front {
                    self.set_stencil_state(
                        vk::StencilFaceFlags::FRONT_AND_BACK,
                        &state.stencil_back,
                    );
                } else {
                    self.set_stencil_state(vk::StencilFaceFlags::FRONT, &state.stencil_front);
                    self.set_stencil_state(vk::StencilFaceFlags::BACK, &state.stencil_back);
                }
            }
        }
    }

    fn set_blend_state(&mut self, state: Option<&BlendInfo>) {
        unsafe {
            if let Some(extended_dynamic_state3) = &self.device.extended_dynamic_state3 {
                if let Some(state) = state {
                    extended_dynamic_state3.cmd_set_color_blend_enable(self.inner, 0, &[vk::TRUE]);

                    let color_blend_equation = vk::ColorBlendEquationEXT::default()
                        .src_color_blend_factor(state.src_color_factor.into())
                        .dst_color_blend_factor(state.dst_color_factor.into())
                        .color_blend_op(state.color_op.into())
                        .src_alpha_blend_factor(state.src_alpha_factor.into())
                        .dst_alpha_blend_factor(state.dst_alpha_factor.into())
                        .alpha_blend_op(state.alpha_op.into());

                    extended_dynamic_state3.cmd_set_color_blend_equation(
                        self.inner,
                        0,
                        &[color_blend_equation],
                    );
                } else {
                    extended_dynamic_state3.cmd_set_color_blend_enable(self.inner, 0, &[vk::FALSE]);
                }
                extended_dynamic_state3.cmd_set_color_write_mask(
                    self.inner,
                    0,
                    &[vk::ColorComponentFlags::RGBA],
                );
            }
        }
    }

    fn dispatch(&mut self, data: PushData, dimensions: UVec3) {
        unsafe {
            self.push_data(data);

            self.device
                .inner
                .cmd_dispatch(self.inner, dimensions[0], dimensions[1], dimensions[2]);
        }
    }

    fn dispatch_indirect(&mut self, data: PushData, indirect_buffer: Self::GpuPtr) {
        unsafe {
            self.push_data(data);
            let heap = self.heap.read().unwrap();
            // Grab VkBuffer from the descriptor heap
            let buffer = heap.ptr_to_buffer(indirect_buffer);

            // TODO: check if buffer is in fact an indirect buffer

            self.device
                .inner
                .cmd_dispatch_indirect(self.inner, buffer.inner, 0);
        }
    }

    fn begin_rendering(&mut self, desc: &RenderingInfo) {
        unsafe {
            let color_attachments: Vec<_> = desc
                .color_attachments
                .iter()
                .map(|target| {
                    let heap = self.heap.read().unwrap();

                    let image = heap.get_image(target.image);
                    let image_view = image.view.unwrap();

                    self.layout_transition_queue.push(LayoutTransition {
                        image: target.image,
                        new_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                        src_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                        src_access_mask: vk::AccessFlags2::empty(),
                        dst_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                        dst_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    });

                    vk::RenderingAttachmentInfo::default()
                        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                        .load_op(target.load_op.into())
                        .store_op(target.store_op.into())
                        .clear_value(target.clear_value.into())
                        .image_view(image_view)
                })
                .collect();

            let depth_attachment = if let Some(ref target) = desc.depth_attachment {
                let heap = self.heap.read().unwrap();
                let image = heap.get_image(target.image);

                self.layout_transition_queue.push(LayoutTransition {
                    image: target.image,
                    new_layout: vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
                    src_stage_mask: vk::PipelineStageFlags2::empty(),
                    src_access_mask: vk::AccessFlags2::empty(),
                    dst_stage_mask: vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS,
                    dst_access_mask: vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ
                        | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                });
                vk::RenderingAttachmentInfo::default()
                    .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                    .load_op(target.load_op.into())
                    .store_op(target.store_op.into())
                    .clear_value(target.clear_value.into())
                    .image_view(image.view.unwrap())
            } else {
                vk::RenderingAttachmentInfo::default()
            };
            let stencil_attachment = if let Some(ref target) = desc.stencil_attachment {
                let heap = self.heap.read().unwrap();
                let image = heap.get_image(target.image);
                self.layout_transition_queue.push(LayoutTransition {
                    image: target.image,
                    new_layout: vk::ImageLayout::STENCIL_ATTACHMENT_OPTIMAL,
                    src_stage_mask: vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
                    src_access_mask: vk::AccessFlags2::empty(),
                    dst_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    dst_access_mask: vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                });
                vk::RenderingAttachmentInfo::default()
                    .image_layout(vk::ImageLayout::STENCIL_ATTACHMENT_OPTIMAL)
                    .load_op(target.load_op.into())
                    .store_op(target.store_op.into())
                    .clear_value(target.clear_value.into())
                    .image_view(image.view.unwrap())
            } else {
                vk::RenderingAttachmentInfo::default()
            };

            let extent = if let Some(target) = desc.color_attachments.first() {
                let [width, height, _] = target.image.dimensions();
                vk::Extent2D { width, height }
            } else {
                panic!();
            };
            let rendering_info = vk::RenderingInfo::default()
                .layer_count(1)
                .view_mask(0)
                .color_attachments(&color_attachments)
                .depth_attachment(&depth_attachment)
                .stencil_attachment(&stencil_attachment)
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D::default(),
                    extent,
                });

            self.multiple_layout_transition(&self.layout_transition_queue);
            self.layout_transition_queue.clear();

            self.device
                .inner
                .cmd_begin_rendering(self.inner, &rendering_info);

            // We save if we
            self.render_pass_state = Some(RenderPassState {
                depth_test: desc.depth_attachment.is_some(),
                stencil_test: desc.stencil_attachment.is_some(),
            });

            self.set_fixed_dynamic_states(extent);
        }
    }

    fn end_rendering(&mut self) {
        unsafe {
            self.render_pass_state = None;

            self.device.inner.cmd_end_rendering(self.inner);
        }
    }

    fn draw(&mut self, data: PushData, indices: u32, instances: u32) {
        unsafe {
            self.push_data(data);

            self.device
                .inner
                .cmd_draw(self.inner, indices, instances, 0, 0);
        }
    }

    fn draw_indexed(&mut self, data: PushData, indices: Self::GpuPtr, instances: u32) {
        unsafe {
            self.push_data(data);

            // Grab from the descriptor heap
            let heap = self.heap.read().unwrap();
            let index_buffer = heap.ptr_to_buffer(indices);
            let index_type = vk::IndexType::UINT32; // Might add support for other index types with some metadata later
            let index_count = index_buffer.len() / 4;

            // TODO: size checks
            let offset = indices.offset;
            let size = indices.size;

            self.device.inner.cmd_bind_index_buffer2(
                self.inner,
                index_buffer.inner,
                offset as u64,
                size as u64,
                index_type,
            );

            self.device
                .inner
                .cmd_draw_indexed(self.inner, index_count as u32, instances, 0, 0, 0);
        }
    }

    fn draw_indexed_indirect(
        &mut self,
        data: PushData,
        indices: Self::GpuPtr,
        indirect: Self::GpuPtr,
    ) {
        unsafe {
            self.push_data(data);

            let heap = self.heap.read().unwrap();
            let index_buffer = heap.ptr_to_buffer(indices);
            let index_type = vk::IndexType::UINT32;

            let index_offset = indices.offset;
            let index_size = indices.size;

            self.device.inner.cmd_bind_index_buffer2(
                self.inner,
                index_buffer.inner,
                index_offset as u64,
                index_size as u64,
                index_type,
            );

            let indirect_buffer = heap.ptr_to_buffer(indirect);

            let indirect_offset = indirect.offset;
            let _indirect_size = indirect.size;

            self.device.inner.cmd_draw_indexed_indirect(
                self.inner,
                indirect_buffer.inner,
                indirect_offset as u64,
                1,
                0,
            );
        }
    }

    fn draw_meshlets(&mut self, data: PushData, dimension: UVec3) {
        self.push_data(&data);

        let mesh = self
            .device
            .mesh_shader
            .as_ref()
            .expect("Mesh shader is not enabled.");

        unsafe {
            mesh.cmd_draw_mesh_tasks(self.inner, dimension[0], dimension[1], dimension[2]);
        }
    }

    fn draw_meshlets_indirect(&mut self, data: PushData, indirect: Self::GpuPtr) {
        unsafe {
            self.push_data(data);

            let mesh = self
                .device
                .mesh_shader
                .as_ref()
                .expect("Mesh shader is not enabled.");

            let heap = self.heap.read().unwrap();

            let indirect_buffer = heap.ptr_to_buffer(indirect);

            let indirect_offset = indirect.offset;
            let _indirect_size = indirect.size;

            mesh.cmd_draw_mesh_tasks_indirect(
                self.inner,
                indirect_buffer.inner,
                indirect_offset as u64,
                1,
                0,
            );
        }
    }
}

impl SwapchainCommandRHI for CommandBuffer {
    fn begin_presenting(&mut self, swapchain_image: GpuImage) {
        let heap = self.heap.read().unwrap();
        let image = heap.get_image(swapchain_image);

        match &image.data {
            ImageData::Allocated(_) => {
                panic!("Cannot present onto an image not acquired from the swapchain")
            }
            ImageData::Swapchain(data) => {
                self.signal.push(SemaphoreInfo {
                    semaphore: data.submit_signal_present_wait.get(),
                    value: 0,
                    stage: Stage::BottomOfPipe,
                });
                self.wait.push(SemaphoreInfo {
                    semaphore: data.submit_wait.get(),
                    value: 0,
                    stage: Stage::ColorAttachmentOutput,
                });
            }
        }
        self.presentation = Some(swapchain_image);
    }
}

pub(super) struct LayoutTransition {
    pub image: GpuImage,
    pub new_layout: vk::ImageLayout,
    pub src_stage_mask: vk::PipelineStageFlags2,
    pub src_access_mask: vk::AccessFlags2,
    pub dst_stage_mask: vk::PipelineStageFlags2,
    pub dst_access_mask: vk::AccessFlags2,
}
