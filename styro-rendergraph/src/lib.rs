use std::marker::PhantomData;
use styro_rhi as rhi;

pub struct RenderGraph {}

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

pub trait LogicalResource {
    type Descriptor: LogicalResourceDescriptor;
}

pub trait LogicalResourceDescriptor {
    type Resource: LogicalResource;
}

pub struct BufferHandle {}
pub struct ImageHandle {}

pub struct Res<T> {
    _marker: PhantomData<T>,
}

pub struct ResourceUsage {}

impl<'graph, 'frame, 'pass> PassBuilder<'graph, 'frame, 'pass> {
    pub fn input_ref<T: LogicalResource>(&mut self, id: Res<T>, usage: ResourceUsage) {
        todo!()
    }

    pub fn output_ref<D: LogicalResourceDescriptor>(
        &mut self,
        desc: D,
        usage: ResourceUsage,
    ) -> Res<D::Resource> {
        todo!()
    }

    pub fn build(self, callback: impl FnOnce(PassContext<'graph, '_>) + 'frame) {
        todo!()
    }
}

impl<'graph, 'frame> Frame<'graph, 'frame> {
    fn new() -> Self {
        todo!()
    }

    pub fn new_pass(&mut self) -> PassBuilder<'graph, 'frame, '_> {
        PassBuilder { frame: self }
    }
}

impl<'graph, 'frame> RenderGraph {
    pub fn new_frame(&'graph mut self) -> Frame<'graph, 'frame> {
        todo!()
    }
}

impl<'graph, 'pass> PassContext<'graph, 'pass> {}

pub struct RenderPass<Push> {
    pipeline: rhi::Pipeline,
    _marker: PhantomData<fn() -> Push>,
}

pub struct ComputePass<Push> {
    pipeline: rhi::Pipeline,
    _marker: PhantomData<fn() -> Push>,
}
