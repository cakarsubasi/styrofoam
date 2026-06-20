use ash::vk;

use crate::renderer::vulkan::{Device, Pipeline};

// This file exists for wankery and saving a few lines of code where it counts

pub(super) unsafe trait ActiveCommandBuffer {
    unsafe fn get_device(&self) -> &Device;

    unsafe fn get_command_buffer(&self) -> vk::CommandBuffer;
}

pub(super) trait DrawCommands: ActiveCommandBuffer {
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

pub(super) trait BindingCommands: ActiveCommandBuffer {
    fn cmd_bind_pipeline(&self, bind_point: vk::PipelineBindPoint, pipeline: &Pipeline) -> &Self {
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

pub(super) trait SynchronizationCommands: ActiveCommandBuffer {
    fn cmd_pipeline_barrier2(&self, dependency_info: &vk::DependencyInfo) -> &Self {
        unsafe {
            self.get_device()
                .inner
                .cmd_pipeline_barrier2(self.get_command_buffer(), dependency_info);
        }
        self
    }
}

pub(super) trait DynamicRenderingCommands: ActiveCommandBuffer {
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

pub(super) trait SynchronizationUtil: SynchronizationCommands {
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
