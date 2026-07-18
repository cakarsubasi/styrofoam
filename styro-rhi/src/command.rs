use ash::vk;
use std::sync::Arc;
use std::sync::RwLock;

use super::device::DescriptorHeap;
use super::device::DeviceHandles;
use super::device::GpuPtr;
use super::device::Semaphore;
use super::swapchain::NextFrame;
use super::*;

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

pub struct CommandBuffer {
    // Handles
    pub(super) device: Arc<DeviceHandles>,
    pub(super) heap: Arc<RwLock<DescriptorHeap>>,
    pub(super) inner: vk::CommandBuffer,
    pub(super) command_pool_idx: u32,
    // Subpass state
    pub(super) wait: Vec<SemaphoreInfo>,
    pub(super) signal: Vec<SemaphoreInfo>,
    pub(super) layout_transition_queue: Vec<LayoutTransition>,
    // Presentation state
    pub(super) present: Option<NextFrame>,
}

// Common helpers
impl CommandBuffer {
    fn push_data(&mut self, data: PushData) {
        unsafe {
            let descriptor_heap = &self.device.descriptor_heap;
            let props = &self.device.descriptor_heap_props;

            assert!(
                props.max_push_data_size >= data.len() as u64,
                "Push data too large"
            );
            // Calling cmd_push_data with a zero length is invalid
            if data.len() > 0 {
                let push_data_info = vk::PushDataInfoEXT::default()
                    .data(vk::HostAddressRangeConstEXT::default().address(data));

                descriptor_heap.cmd_push_data(self.inner, &push_data_info);
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
            return;
        }
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
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
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
        for transition in transitions {
            let LayoutTransition {
                image,
                new_layout,
                src_stage_mask,
                src_access_mask,
                dst_stage_mask,
                dst_access_mask,
            } = transition;

            // TODO: yeet the Framebuffer thing, and emit a single barrier command
            let (image, old_layout) = match image {
                Framebuffer::Image(gpu_ptr) => {
                    let guard = self.heap.read().unwrap();
                    let image = guard.ptr_to_image(*gpu_ptr);
                    let old_layout = image.current_layout.get();
                    image.current_layout.set(*new_layout);
                    (image.inner, old_layout)
                }
                Framebuffer::Swapchain(swapchain_image) => {
                    let image = swapchain_image.image;
                    let old_layout = if *new_layout == vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL {
                        vk::ImageLayout::UNDEFINED
                    } else {
                        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
                    };
                    (image, old_layout)
                }
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
        }
    }
}

impl CommandRHI for CommandBuffer {
    type GpuPtr = GpuPtr;
    type Pipeline = super::Pipeline;
    type Semaphore = Semaphore;

    fn mem_cpy(&mut self, dst: Self::GpuPtr, src: Self::GpuPtr) {
        let heap = self.heap.read().unwrap();
        let src_buffer = heap.ptr_to_buffer(src);
        let dst_buffer = heap.ptr_to_buffer(dst);

        let src_size = src_buffer.len();
        let dst_size = dst_buffer.len();

        assert!(dst_size >= src_size);
        // TODO: we might wish to have more flexible copying
        let info = [vk::BufferCopy::default()
            .src_offset(0)
            .dst_offset(0)
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

    fn copy_to_texture(&mut self, _dst: Self::GpuPtr, _src: Self::GpuPtr) {
        todo!();
    }

    fn bind_descriptor_heap(&mut self, resource_heap: Self::GpuPtr, sampler_heap: Self::GpuPtr) {
        let resource_heap_null = resource_heap.is_null();
        let sampler_heap_null = sampler_heap.is_null();

        let heap = self.heap.read().unwrap();

        let descriptor_heap = &self.device.descriptor_heap;
        let props = &self.device.descriptor_heap_props;

        if !resource_heap_null {
            let resource_heap_buf = heap.ptr_to_buffer(resource_heap);

            let resource_addr = unsafe {
                self.device.inner.get_buffer_device_address(
                    &vk::BufferDeviceAddressInfo::default().buffer(resource_heap_buf.inner),
                )
            };

            let resource_heap_size = resource_heap_buf.len();

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
                )
            };

            let sampler_heap_size = sampler_heap_buf.len();

            let heap_bind_info = vk::BindHeapInfoEXT::default()
                .heap_range(
                    vk::DeviceAddressRangeKHR::default()
                        .address(sampler_addr)
                        .size(sampler_heap_size),
                )
                .reserved_range_offset(sampler_heap_size - props.min_sampler_heap_reserved_range)
                .reserved_range_size(props.min_sampler_heap_reserved_range);

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

    fn barrier(&mut self, before: Stage, after: Stage /* something goes here */) {
        unsafe {
            // A read after write indicates a true dependency which is the strictest
            // synchronization option we have. Might be a good idea to make this more relaxed
            let barriers = [vk::MemoryBarrier2::default()
                .src_stage_mask(before)
                .src_access_mask(vk::AccessFlags2::MEMORY_WRITE)
                .dst_stage_mask(after)
                .dst_access_mask(vk::AccessFlags2::MEMORY_READ)];

            let dependency_info = vk::DependencyInfo::default().memory_barriers(&barriers);
            self.device
                .inner
                .cmd_pipeline_barrier2(self.inner, &dependency_info);
        }
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

    fn set_depth_stencil_state(&mut self, _state: DepthStencilState) {
        todo!()
    }

    fn set_blend_state(&mut self, state: BlendState) {
        unsafe {
            let extended_dynamic_state3 = &self.device.extended_dynamic_state3;

            extended_dynamic_state3.cmd_set_color_blend_enable(self.inner, 0, &[vk::TRUE]);

            let color_blend_equation = vk::ColorBlendEquationEXT::default()
                .src_color_blend_factor(state.src_color_factor)
                .dst_color_blend_factor(state.dst_color_factor)
                .color_blend_op(state.color_op)
                .src_alpha_blend_factor(state.src_alpha_factor)
                .dst_alpha_blend_factor(state.dst_alpha_factor)
                .alpha_blend_op(state.alpha_op);

            extended_dynamic_state3.cmd_set_color_blend_equation(
                self.inner,
                0,
                &[color_blend_equation],
            );
        }
    }

    fn gpu_dispatch(&mut self, data: PushData, dimensions: UVec3) {
        unsafe {
            self.push_data(data);

            self.device
                .inner
                .cmd_dispatch(self.inner, dimensions[0], dimensions[1], dimensions[2]);
        }
    }

    fn gpu_dispatch_indirect(&mut self, data: PushData, indirect_buffer: Self::GpuPtr) {
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

    fn begin_render_pass(&mut self, desc: &RenderPassDescription) {
        unsafe {
            let color_attachments: Vec<_> = desc
                .color_targets
                .iter()
                .map(|target| {
                    let heap = self.heap.read().unwrap();

                    let image_view = match target.image {
                        Framebuffer::Image(gpu_ptr) => {
                            let image = heap.ptr_to_image(gpu_ptr);
                            image.view.unwrap()
                        }
                        Framebuffer::Swapchain(SwapchainImage {
                            image: _image,
                            view,
                            extent: _,
                            format: _,
                        }) => view,
                    };

                    self.layout_transition_queue.push(LayoutTransition {
                        image: target.image,
                        new_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                        src_stage_mask: vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
                        src_access_mask: vk::AccessFlags2::empty(),
                        dst_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                        dst_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    });

                    vk::RenderingAttachmentInfo::default()
                        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                        .load_op(target.load_op)
                        .store_op(target.store_op)
                        .clear_value(target.clear_value)
                        .image_view(image_view)
                })
                .collect();
            let color_attachments = if let Some(ref presentation) = self.present {
                let swapchain_view = presentation.image.view;
                self.layout_transition_queue.push(LayoutTransition {
                    image: Framebuffer::Swapchain(presentation.image),
                    new_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    src_stage_mask: vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
                    src_access_mask: vk::AccessFlags2::empty(),
                    dst_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    dst_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                });
                vec![
                    vk::RenderingAttachmentInfo::default()
                        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                        .load_op(vk::AttachmentLoadOp::CLEAR)
                        .store_op(vk::AttachmentStoreOp::STORE)
                        .clear_value(vk::ClearValue {
                            color: Default::default(),
                        })
                        .image_view(swapchain_view),
                ]
            } else {
                color_attachments
            };

            let depth_attachment = if let Some(ref target) = desc.depth_target {
                let heap = self.heap.read().unwrap();
                let image = match target.image {
                    Framebuffer::Image(gpu_ptr) => {
                        let image = heap.ptr_to_image(gpu_ptr);

                        image
                    }
                    _ => panic!("Can't use swapchain images as depth attachment"),
                };

                self.layout_transition_queue.push(LayoutTransition {
                    image: target.image,
                    new_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    src_stage_mask: vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
                    src_access_mask: vk::AccessFlags2::empty(),
                    dst_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    dst_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                });
                vk::RenderingAttachmentInfo::default()
                    .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
                    .load_op(target.load_op)
                    .store_op(target.store_op)
                    .clear_value(target.clear_value)
                    .image_view(image.view.unwrap())
            } else {
                vk::RenderingAttachmentInfo::default()
            };
            let stencil_attachment = if let Some(ref target) = desc.stencil_target {
                let heap = self.heap.read().unwrap();
                let image = match target.image {
                    Framebuffer::Image(gpu_ptr) => heap.ptr_to_image(gpu_ptr),
                    _ => panic!("Can't use swapchain images as stencil attachment"),
                };
                self.layout_transition_queue.push(LayoutTransition {
                    image: target.image,
                    new_layout: vk::ImageLayout::STENCIL_ATTACHMENT_OPTIMAL,
                    src_stage_mask: vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
                    src_access_mask: vk::AccessFlags2::empty(),
                    dst_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    dst_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                });
                vk::RenderingAttachmentInfo::default()
                    .image_layout(vk::ImageLayout::STENCIL_ATTACHMENT_OPTIMAL)
                    .load_op(target.load_op)
                    .store_op(target.store_op)
                    .clear_value(target.clear_value)
                    .image_view(image.view.unwrap())
            } else {
                vk::RenderingAttachmentInfo::default()
            };

            let extent = if let Some(target) = desc.color_targets.first() {
                target.image.extent()
            } else {
                // Swapchain extent
                self.present.as_ref().unwrap().image.extent
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

            //
            self.set_fixed_dynamic_states(extent);
        }
    }

    fn end_render_pass(&mut self) {
        unsafe {
            self.device.inner.cmd_end_rendering(self.inner);
        }
    }

    fn draw_indexed_instanced(&mut self, data: PushData, indices: Self::GpuPtr, instances: u32) {
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

    fn draw_indexed_instanced_indirect(
        &mut self,
        _data: PushData,
        _indices: Self::GpuPtr,
        _indirect: Self::GpuPtr,
    ) {
        todo!()
    }

    fn draw_meshlets(&mut self, _data: PushData, _dimension: UVec3) {
        todo!()
    }

    fn draw_meshlets_indirect(&mut self, _data: PushData, _dim_data: Self::GpuPtr) {
        todo!()
    }
}

pub(super) struct LayoutTransition {
    image: Framebuffer,
    new_layout: vk::ImageLayout,
    src_stage_mask: vk::PipelineStageFlags2,
    src_access_mask: vk::AccessFlags2,
    dst_stage_mask: vk::PipelineStageFlags2,
    dst_access_mask: vk::AccessFlags2,
}

impl Framebuffer {
    fn extent(&self) -> vk::Extent2D {
        todo!()
    }
}
