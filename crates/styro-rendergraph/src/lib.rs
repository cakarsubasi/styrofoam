use bytemuck::NoUninit;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use std::{collections::BTreeMap, marker::PhantomData};
use styro_rhi::{self as rhi, SwapchainDeviceRHI, SwapchainRHI, traits::*};

use crate::resource::{
    BufferHandle, ImageHandle, LogicalResource, LogicalResourceDescriptor, Res, ResourceUsage,
    Sampler, get_usage,
};

//mod caches;
pub mod pass;
pub mod resource;

// Derive already gives the right order
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct EdgeKey {
    input_pass: usize,
    output_pass: usize,
}

struct Dependency {
    input_dependency: (),
    output_dependency: (),
}

type GraphEdges = BTreeMap<EdgeKey, Dependency>;

struct GraphResources {}

pub struct RenderGraph {
    device: rhi::Device,
    _graphics_queue: rhi::Queue,
    _swapchain: rhi::Swapchain,
    caches: Vec<GraphResources>,
    frame_index: u64,
}

pub struct Frame<'graph, 'frame> {
    _graph: &'graph mut RenderGraph,
    passes: Vec<Box<dyn FnOnce(PassContext<'graph, '_>) + 'frame>>,
    ref_id_mapping: Vec<usize>,
    res_pass_mapping: Vec<usize>, // Res<id> -> Output
    edges: GraphEdges,
    _frame: PhantomData<&'frame ()>,
}

pub struct PassBuilder<'graph, 'frame, 'pass> {
    frame: &'pass mut Frame<'graph, 'frame>,
}

pub struct PassContext<'graph, 'pass> {
    pub command_buffer: &'pass mut rhi::CommandBuffer,
    _graph: &'pass mut RenderGraph,
    _pass: PhantomData<&'graph ()>,
}

impl<'graph, 'frame, 'pass> PassBuilder<'graph, 'frame, 'pass> {
    pub fn input_ref<T: LogicalResource>(&mut self, id: Res<T>, usage: impl ResourceUsage) {
        let input_pass = self.frame.res_pass_mapping[id.id];
        let output_pass = self.frame.passes.len();
        if let Some(dependency) = self.frame.edges.get_mut(&EdgeKey {
            input_pass,
            output_pass,
        }) {
            // merge usages here
        } else {
            self.frame.edges.insert(
                EdgeKey {
                    input_pass,
                    output_pass,
                },
                Dependency {
                    input_dependency: (),
                    output_dependency: (),
                },
            );
        }
    }

    pub fn output_ref<D: LogicalResourceDescriptor>(
        &mut self,
        desc: D,
        usage: impl ResourceUsage,
    ) -> Res<D::Resource> {
        let pass = self.frame.passes.len();
        let usage = get_usage(usage);
        let resource = desc.to_resource();
        // TODO: need to forge the IDs here, rather than grabbing them
        let id = resource.id();
        self.frame.ref_id_mapping.push(id);
        self.frame.res_pass_mapping.push(pass);
        unsafe {
            let res = Res::from_idx(id);
            res
        }
    }

    pub fn build(self, callback: impl FnOnce(PassContext<'graph, '_>) + 'frame) {
        self.frame.passes.push(Box::new(callback));
    }
}

fn compile<Vertex, Edge>(nodes: Vec<Vertex>, edges: BTreeMap<EdgeKey, Edge>) -> Vec<Vertex> {
    // Step 1: figure out starting nodes

    let mut visited = vec![false; nodes.len()];
    for (edge, _) in &edges {
        visited[edge.output_pass] = true;
    }
    let mut starting = vec![];
    for (idx, node) in visited.iter().enumerate() {
        if *node {
            starting.push(idx);
        }
    }
    // Step 2: Reorder
    visited.fill(false);

    let mut sorted = vec![0; nodes.len()];

    for node in starting {
        sorted.push(node);
        for (key, _) in edges.range(
            EdgeKey {
                input_pass: node,
                output_pass: 0,
            }..EdgeKey {
                input_pass: node + 1,
                output_pass: 0,
            },
        ) {
            // TODO
        }
    }

    todo!()
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
        // Do nothing for now
        //todo!()
    }

    pub fn run(self) {
        let semaphore = self._graph.device_mut().create_semaphore(0);
        let passes = self.passes;
        // Compile frame

        // block on completion of frame (n - FRAMES_IN_FLIGHT)

        // Grab new swapchain image

        // Perform passes
        for (idx, pass) in passes.into_iter().enumerate() {
            let mut command_buffer = self._graph._graphics_queue.begin_recording(0);
            pass(PassContext {
                command_buffer: &mut command_buffer,
                _graph: self._graph,
                _pass: PhantomData,
            });
            // crude but correct after topological sort
            command_buffer.wait_before(rhi::Stage::TopOfPipe, &semaphore, idx as u64);
            command_buffer.signal_after(rhi::Stage::BottomOfPipe, &semaphore, idx as u64 + 1);
            unsafe {
                self._graph._graphics_queue.submit(&[command_buffer]);
            }
        }
    }
}

impl<'graph, 'frame> RenderGraph {
    pub fn new(display_handle: RawDisplayHandle, window_handle: RawWindowHandle) -> Self {
        const FRAMES_IN_FLIGHT: u32 = 2;
        let window_system = rhi::window::WindowSystem::from(display_handle);
        let mut device = rhi::Device::new(&rhi::DeviceCreateInfo {
            device_idx: 0,
            device_features: rhi::DeviceFeatures {
                descriptor_heap: true,
                push_descriptor: false,
                dynamic_blend_state: true,
                mesh_shader: true,
            },
            presentation: Some(window_system),
        });
        // TODO: multiple queues
        let graphics_queue = device.create_queue(rhi::QueueType::Graphics, FRAMES_IN_FLIGHT * 4, 8);
        // TODO: swapchain extents, and better format selection
        let swapchain = unsafe {
            device
                .create_swapchain(
                    &graphics_queue,
                    &rhi::WindowSystemData::from_raw_handles(display_handle, window_handle),
                    &rhi::SwapchainInfo {
                        size: FRAMES_IN_FLIGHT,
                        format: rhi::Format::R8G8B8A8_Srgb,
                        color_space: rhi::ColorSpace::SrgbNonlinear,
                        preferred_extent: [0, 0],
                    },
                )
                .unwrap()
        };
        Self {
            device,
            _graphics_queue: graphics_queue,
            _swapchain: swapchain,
            caches: vec![],
            frame_index: 0,
        }
    }

    pub fn new_frame(&'graph mut self) -> Frame<'graph, 'frame> {
        let swapchain_image = self._swapchain.acquire_next_image().unwrap();

        Frame {
            _graph: self,
            passes: vec![],
            _frame: PhantomData,
            ref_id_mapping: vec![],
            res_pass_mapping: vec![],
            edges: BTreeMap::new(),
        }
    }

    pub fn device_mut(&mut self) -> &mut rhi::Device {
        &mut self.device
    }
}

impl<'graph, 'pass> PassContext<'graph, 'pass> {
    pub fn get<T: 'pass>(&self, resource: Res<T>) -> T {
        todo!()
    }

    pub fn copy_to_buffer<T: NoUninit>(&mut self, buffer: Res<BufferHandle>, data: &[T]) {
        let buffer: &mut [u8] = todo!();
        let data_bytes: &[u8] = bytemuck::cast_slice(data);
        buffer.copy_from_slice(data_bytes);
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
