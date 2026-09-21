use rhi::traits::*;
use std::{marker::PhantomData, sync::Arc};
use styro_rhi as rhi;

use crate::{PassContext, RenderPass, resource::BufferHandle};

pub struct GraphicsPipeline<Push> {
    pipeline: Arc<rhi::Pipeline>,
    _marker: PhantomData<fn() -> Push>,
}

impl<Push: bytemuck::NoUninit> GraphicsPipeline<Push> {
    pub fn new(
        device: &mut rhi::Device,
        vertex: &rhi::ShaderIR,
        fragment: &rhi::ShaderIR,
        raster_desc: &rhi::RasterDescription,
    ) -> Self {
        let pipeline = unsafe { device.create_graphics_pipeline(vertex, fragment, raster_desc) };
        Self {
            pipeline: Arc::new(pipeline),
            _marker: PhantomData,
        }
    }

    pub fn draw(
        &self,
        ctx: &mut RenderPass<'_, '_, '_>,
        data: &Push,
        num_indices: u32,
        num_instances: u32,
    ) {
        let data = bytemuck::bytes_of(data);
        ctx.context.command_buffer.set_pipeline(&self.pipeline);
        ctx.context
            .command_buffer
            .draw(data, num_indices, num_instances);
    }

    pub fn draw_indexed(
        &self,
        ctx: &mut RenderPass<'_, '_, '_>,
        data: &Push,
        index_buffer: BufferHandle,
        num_instances: u32,
    ) {
        let data = bytemuck::bytes_of(data);
        let indices = todo!(); // index_buffer.ptr().addr();
        ctx.context.command_buffer.set_pipeline(&self.pipeline);
        ctx.context
            .command_buffer
            .draw_indexed(data, indices, num_instances);
    }
}

pub struct ComputePass<Push> {
    pipeline: Arc<rhi::Pipeline>,
    _marker: PhantomData<fn() -> Push>,
}

impl<Push: bytemuck::NoUninit> ComputePass<Push> {
    pub fn new(device: &mut rhi::Device, ir: &rhi::ShaderIR) -> Self {
        let pipeline = unsafe { device.create_compute_pipeline(ir) };
        Self {
            pipeline: Arc::new(pipeline),
            _marker: PhantomData,
        }
    }

    pub fn dispatch(&self, ctx: &mut PassContext<'_, '_>, data: &Push, dimensions: &[u32; 3]) {
        let data = bytemuck::bytes_of(data);
        ctx.command_buffer.set_pipeline(&self.pipeline);
        ctx.command_buffer.dispatch(data, *dimensions);
    }
}
