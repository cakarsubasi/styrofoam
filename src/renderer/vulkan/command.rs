use ash::VkResult;
use ash::vk;
use std::sync::Arc;

use super::*;
use crate::renderer::shader::reflect::ShaderInfo;

pub struct CommandPool {
    pub(super) device: Arc<Device>,
    pub(super) inner: vk::CommandPool,
    pub(super) command_buffers: Vec<CommandBuffer>,
}

impl CommandPool {
    pub fn allocate_command_buffers(&mut self, number_of_command_buffers: u32) -> VkResult<()> {
        let allocate_info = vk::CommandBufferAllocateInfo::default()
            .command_buffer_count(number_of_command_buffers)
            .command_pool(self.inner);

        unsafe {
            let mut command_buffers = self
                .device
                .inner
                .allocate_command_buffers(&allocate_info)?
                .into_iter()
                .map(|cb| CommandBuffer {
                    device: Arc::clone(&self.device),
                    inner: cb,
                })
                .collect::<Vec<_>>();
            self.command_buffers.append(&mut command_buffers);
        }
        Ok(())
    }

    // grab next command buffer for rendering
    pub fn acquire_command_buffer(&self, frame_index: u64) -> &CommandBuffer {
        let idx = frame_index as usize % self.command_buffers.len();

        &self.command_buffers[idx]
    }
}

impl Drop for CommandPool {
    fn drop(&mut self) {
        unsafe {
            self.device.inner.destroy_command_pool(self.inner, None);
        }
    }
}

pub struct Pipeline {
    pub(super) device: Arc<Device>,
    pub(super) inner: vk::Pipeline,
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

impl ShaderModule {}

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

impl Drop for ShaderModule {
    fn drop(&mut self) {
        unsafe {
            self.device
                .inner
                .destroy_shader_module(self.shader_module, None);
        }
    }
}

pub struct CommandBuffer {
    pub(super) device: Arc<Device>,
    // interesting question. We could store an Arc to CommandPool and have it free after all
    // command buffers or own all command buffers within CommandPool and destroy CommandBuffers when CommandPool is dropped
    pub(super) inner: vk::CommandBuffer,
}
unsafe impl ActiveCommandBuffer for CommandBuffer {
    unsafe fn get_device(&self) -> &Device {
        &self.device
    }

    unsafe fn get_command_buffer(&self) -> vk::CommandBuffer {
        self.inner
    }
}
impl SynchronizationCommands for CommandBuffer {}

pub struct RecordingCommandBuffer<'a> {
    inner: &'a CommandBuffer,
}

impl RecordingCommandBuffer<'_> {
    pub fn render(&self, rendering_info: &vk::RenderingInfo, f: impl Fn(&Self)) {
        self.cmd_begin_rendering(rendering_info);
        f(self);
        self.cmd_end_rendering();
    }
}

unsafe impl ActiveCommandBuffer for RecordingCommandBuffer<'_> {
    unsafe fn get_device(&self) -> &Device {
        &self.inner.device
    }

    unsafe fn get_command_buffer(&self) -> vk::CommandBuffer {
        self.inner.inner
    }
}
impl DrawCommands for RecordingCommandBuffer<'_> {}
impl DynamicRenderingCommands for RecordingCommandBuffer<'_> {}
impl BindingCommands for RecordingCommandBuffer<'_> {}
impl SynchronizationCommands for RecordingCommandBuffer<'_> {}

impl CommandBuffer {
    pub fn reset(&self) -> VkResult<()> {
        unsafe {
            self.device
                .inner
                .reset_command_buffer(self.inner, vk::CommandBufferResetFlags::RELEASE_RESOURCES)
        }
    }

    pub fn record(&self, f: impl Fn(&RecordingCommandBuffer)) -> VkResult<()> {
        unsafe {
            self.reset()?;

            let begin_info = &vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

            self.device
                .inner
                .begin_command_buffer(self.inner, begin_info)?;

            f(&RecordingCommandBuffer { inner: self });

            self.device.inner.end_command_buffer(self.inner)?;
        }
        Ok(())
    }
}

impl Drop for CommandBuffer {
    fn drop(&mut self) {
        // hmmm
    }
}

pub mod traits {
    use super::*;

    // This module exists for wankery and saving a few lines of code where it counts
    pub unsafe trait ActiveCommandBuffer {
        unsafe fn get_device(&self) -> &Device;

        unsafe fn get_command_buffer(&self) -> vk::CommandBuffer;
    }

    pub trait DrawCommands: ActiveCommandBuffer {
        fn cmd_draw(
            &self,
            vertex_count: u32,
            instance_count: u32,
            first_vertex: u32,
            first_instance: u32,
        ) -> &Self {
            unsafe {
                self.get_device().inner.cmd_draw(
                    self.get_command_buffer(),
                    vertex_count,
                    instance_count,
                    first_vertex,
                    first_instance,
                );
            }
            self
        }

        //    fn CmdDrawIndexed(
        //        &self,
        //        index_count: u32,
        //        vertex_count: u32,
        //        instance_count: u32,
        //        first_vertex: u32,
        //        first_instance: u32,
        //    ) -> &self;
        //
        //    fn CmdDrawIndirect(
        //        &self,
        //        buffer: &IndirectBuffer,
        //        offset: DeviceSize,
        //        draw_count: u32,
        //        stride: u32,
        //    );
        //
        //    fn CmdDrawIndexedIndirect(
        //        &self,
        //        buffer: &IndirectBuffer,
        //        offset: DeviceSize,
        //        draw_count: u32,
        //        stride: u32,
        //    ) -> &self;
        //
        //    fn CmdDrawIndexedIndirectCount(
        //        &self,
        //        buffer: &IndirectBuffer,
        //        offset: DeviceSize,
        //        count_buffer: &IndirectBuffer,
        //        count_buffer_offset: DeviceSize,
        //        max_draw_count: u32,
        //        stride: u32,
        //    ) -> &self;
    }

    pub trait BindingCommands: ActiveCommandBuffer {
        fn cmd_bind_pipeline(
            &self,
            bind_point: vk::PipelineBindPoint,
            pipeline: &Pipeline,
        ) -> &Self {
            unsafe {
                self.get_device().inner.cmd_bind_pipeline(
                    self.get_command_buffer(),
                    bind_point,
                    pipeline.inner,
                );
            }
            self
        }
    }

    pub trait SynchronizationCommands: ActiveCommandBuffer {
        fn cmd_pipeline_barrier2(&self, dependency_info: &vk::DependencyInfo) -> &Self {
            unsafe {
                self.get_device()
                    .inner
                    .cmd_pipeline_barrier2(self.get_command_buffer(), dependency_info);
            }
            self
        }
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

    pub trait SynchronizationUtil: SynchronizationCommands {
        fn transition_image_layout(
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
                .src_queue_family_index(unsafe { self.get_device() }.queue_family_index)
                .dst_queue_family_index(unsafe { self.get_device() }.queue_family_index)
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
            self.cmd_pipeline_barrier2(&dependency_info);
        }
    }

    impl<T> SynchronizationUtil for T where T: SynchronizationCommands {}
}
