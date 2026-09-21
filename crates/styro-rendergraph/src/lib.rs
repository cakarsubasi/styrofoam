use bytemuck::NoUninit;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use std::{marker::PhantomData, sync::Arc};
use styro_rhi::{self as rhi, traits::*};

use crate::resource::{
    BufferHandle, ImageHandle, LogicalResource, LogicalResourceDescriptor, Res, ResourceUsage,
    Sampler,
};

pub mod pass;
pub mod resource;

pub struct RenderGraph {
    _priv: (),
}

pub struct Frame<'graph, 'frame> {
    _graph: PhantomData<&'graph ()>,
    _frame: PhantomData<&'frame ()>,
}

pub struct PassBuilder<'graph, 'frame, 'pass> {
    frame: &'pass mut Frame<'graph, 'frame>,
}

pub struct PassContext<'graph, 'pass> {
    pub command_buffer: rhi::CommandBuffer,
    _graph: PhantomData<&'graph ()>,
    _pass: PhantomData<&'pass ()>,
}

impl<'graph, 'frame, 'pass> PassBuilder<'graph, 'frame, 'pass> {
    pub fn input_ref<T: LogicalResource>(&mut self, id: Res<T>, usage: impl ResourceUsage) {
        todo!()
    }

    pub fn output_ref<D: LogicalResourceDescriptor>(
        &mut self,
        desc: D,
        usage: impl ResourceUsage,
    ) -> Res<D::Resource> {
        todo!()
    }

    pub fn build(self, callback: impl FnOnce(PassContext<'graph, '_>) + 'frame) {
        todo!()
    }
}

impl<'graph, 'frame> Frame<'graph, 'frame> {
    pub fn new_pass(&mut self, name: &str) -> PassBuilder<'graph, 'frame, '_> {
        PassBuilder { frame: self }
    }

    pub fn sampler(&mut self, info: &rhi::SamplerInfo) -> Sampler {
        todo!()
    }

    pub fn swapchain_image(&self) -> Res<ImageHandle> {
        todo!()
    }

    pub fn delete<T: LogicalResource>(&mut self, resource: Res<T>) {
        todo!()
    }
}

impl<'graph, 'frame> RenderGraph {
    pub fn new(display_handle: RawDisplayHandle, window_handle: RawWindowHandle) -> Self {
        Self { _priv: () }
    }

    pub fn new_frame(&'graph mut self) -> Frame<'graph, 'frame> {
        todo!()
    }

    pub fn device_mut(&mut self) -> &mut rhi::Device {
        todo!()
    }
}

impl<'graph, 'pass> PassContext<'graph, 'pass> {
    pub fn get<T: 'pass>(&self, resource: Res<T>) -> T {
        todo!()
    }

    pub fn copy_to_buffer<T: NoUninit>(&self, buffer: Res<BufferHandle>, data: &[T]) {
        todo!()
    }

    pub fn copy_to_image<T: NoUninit>(
        &mut self,
        image: Res<ImageHandle>,
        data: &[T], /* something goes here */
    ) {
        todo!()
    }

    pub fn render_pass(
        &mut self,
        color_attachments: &[Attachment],
        depth_attachment: Option<&Attachment>,
        stencil_attachment: Option<&Attachment>,
    ) -> RenderPass<'graph, '_, 'pass> {
        // TODO: arena
        let color_attachments: Vec<_> = color_attachments.iter().map(|a| a.to_rhi()).collect();

        let rendering_info = rhi::RenderingInfo {
            color_attachments: &color_attachments,
            depth_attachment: depth_attachment.map(|a| a.to_rhi()),
            stencil_attachment: stencil_attachment.map(|a| a.to_rhi()),
        };

        self.command_buffer.begin_rendering(&rendering_info);
        RenderPass { context: self }
    }
}

pub struct RenderPass<'graph, 'frame, 'pass> {
    context: &'frame mut PassContext<'graph, 'pass>,
}

impl RenderPass<'_, '_, '_> {}

pub enum Load {
    Load,
    Clear(f32, f32, f32, f32),
    DontCare,
}

pub struct Attachment {
    pub image: Res<ImageHandle>,
    pub load: Load,
    pub store: bool,
}

impl Attachment {
    fn to_rhi(&self) -> rhi::Attachment {
        rhi::Attachment {
            image: todo!(),
            load_op: todo!(),
            store_op: todo!(),
            clear_value: todo!(),
        }
    }
}
