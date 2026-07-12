use ash::VkResult;
use ash::vk;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::Weak;

use super::*;
use crate::renderer::shader::reflect::ShaderInfo;
use crate::renderer::vulkan::device::DescriptorHeap;
use crate::renderer::vulkan::device::DeviceHandles;
use crate::renderer::vulkan::device::GpuPtr;
use crate::renderer::vulkan::device::TimelineSemaphore;

pub enum PipelineType {
    Graphics,
    Compute,
    RayTracing,
    Mesh,
}

impl PipelineType {
    fn bind_point(&self) -> vk::PipelineBindPoint {
        match self {
            PipelineType::Graphics => vk::PipelineBindPoint::GRAPHICS,
            PipelineType::Compute => vk::PipelineBindPoint::COMPUTE,
            PipelineType::RayTracing => vk::PipelineBindPoint::RAY_TRACING_KHR,
            PipelineType::Mesh => vk::PipelineBindPoint::GRAPHICS,
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

pub struct ShaderModule {
    pub(super) device: Arc<Device>,
    pub(super) shader_module: vk::ShaderModule,
    pub(super) info: Vec<ShaderInfo>,
}

impl Drop for ShaderModule {
    fn drop(&mut self) {
        unsafe {
            self.device
                .inner
                .destroy_shader_module(self.shader_module, None);
        }
    }
}

pub struct ImageView {
    pub(super) device: Arc<Device>,
    pub(super) inner: vk::ImageView,
    // image: Arc<vk::Image>, // Maybe Weak would be better?
    // Or maybe a different representation would be better entirely
    // like OwningView which owns the image
}

impl Drop for ImageView {
    fn drop(&mut self) {
        unsafe {
            self.device.inner.destroy_image_view(self.inner, None);
        }
    }
}

pub(super) struct SemaphoreInfo {
    pub(super) semaphore: vk::Semaphore,
    pub(super) value: u64,
    pub(super) stage: Stage,
}

pub struct PresentSubmitEtc {
    pub(super) image_idx: u32,
    pub(super) swapchain_extent: vk::Extent2D,
    pub(super) semaphore: vk::Semaphore,
}

pub struct CommandBuffer {
    pub(super) device: Arc<DeviceHandles>,
    pub(super) heap: Arc<RwLock<DescriptorHeap>>, // I need to have some sort of RW lock but for now, arc is ok
    pub(super) inner: vk::CommandBuffer,
    pub(super) wait: Vec<SemaphoreInfo>,
    pub(super) signal: Vec<SemaphoreInfo>,
    pub(super) layout_transition_queue: Vec<LayoutTransition>,
    pub(super) present: Option<PresentSubmitEtc>,
}

pub mod traits {

    use super::*;

    // This module exists for wankery and saving a few lines of code where it counts
    pub unsafe trait ActiveCommandBuffer {
        unsafe fn get_device(&self) -> &Device;

        unsafe fn get_command_buffer(&self) -> vk::CommandBuffer;
    }

    pub trait DynamicRenderingCommands: ActiveCommandBuffer {
        fn cmd_begin_rendering(&self, info: &vk::RenderingInfo) -> &Self {
            unsafe {
                self.get_device()
                    .inner
                    .cmd_begin_rendering(self.get_command_buffer(), info);
            }
            self
        }

        fn cmd_end_rendering(&self) -> &Self {
            unsafe {
                self.get_device()
                    .inner
                    .cmd_end_rendering(self.get_command_buffer());
            }
            self
        }

        fn cmd_set_scissor(&self, scissors: &[vk::Rect2D]) -> &Self {
            unsafe {
                self.get_device()
                    .inner
                    .cmd_set_scissor(self.get_command_buffer(), 0, scissors);
            }
            self
        }

        fn cmd_set_viewport(&self, viewports: &[vk::Viewport]) -> &Self {
            unsafe {
                self.get_device()
                    .inner
                    .cmd_set_viewport(self.get_command_buffer(), 0, viewports);
            }
            self
        }
    }
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

    unsafe fn transition_image_layout(
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
        self.device
            .inner
            .cmd_pipeline_barrier2(self.inner, &dependency_info);
    }

    unsafe fn multiple_layout_transition(&self, transitions: &[LayoutTransition]) {
        for transition in transitions {
            let LayoutTransition {
                image,
                new_layout,
                src_stage_mask,
                src_access_mask,
                dst_stage_mask,
                dst_access_mask,
            } = transition;

            let (image, old_layout) = match image {
                Framebuffer::Image(gpu_ptr) => {
                    let guard = self.heap.read().unwrap();
                    let image = guard.ptr_to_image(*gpu_ptr);
                    let old_layout = image.current_layout.get();
                    image.current_layout.set(*new_layout);
                    (image.inner, old_layout)
                }
                Framebuffer::Swapchain(swapchain_image) => {
                    swapchain_image.image;
                    todo!()
                }
            };

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
    type Semaphore = TimelineSemaphore;
    type Pipeline = super::Pipeline;

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

    fn set_pipeline(&mut self, pipeline: &Self::Pipeline) {
        let bind_point = pipeline.ty.bind_point();
        unsafe {
            self.device
                .inner
                .cmd_bind_pipeline(self.inner, bind_point, pipeline.inner);
        }
    }

    fn set_depth_stencil_state(&mut self, state: DepthStencilState) {
        todo!()
    }

    fn set_blend_state(&mut self, state: BlendState) {
        todo!()
    }

    fn gpu_dispatch(&mut self, data: PushData, dimensions: U32_3) {
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
                self.present.as_ref().unwrap().swapchain_extent
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

            self.device
                .inner
                .cmd_begin_rendering(self.inner, &rendering_info);

            self.multiple_layout_transition(&self.layout_transition_queue);
            self.layout_transition_queue.clear();

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

            self.device
                .inner
                .cmd_bind_index_buffer(self.inner, index_buffer.inner, 0, index_type);

            self.device
                .inner
                .cmd_draw_indexed(self.inner, index_count as u32, instances, 0, 0, 0);
        }
    }

    fn draw_indexed_instanced_indirect(
        &mut self,
        data: PushData,
        indices: Self::GpuPtr,
        indirect: Self::GpuPtr,
    ) {
        todo!()
    }

    fn draw_meshlets(&mut self, data: PushData, dimension: U32_3) {
        todo!()
    }

    fn draw_meshlets_indirect(&mut self, data: PushData, dim_data: Self::GpuPtr) {
        todo!()
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
}

pub struct LayoutTransition {
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
