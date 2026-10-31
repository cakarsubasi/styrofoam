use bytemuck::NoUninit;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use std::{
    any::Any,
    collections::BTreeMap,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::swap,
};
use styro_rhi::{self as rhi, SwapchainDeviceRHI, SwapchainRHI, traits::*};

use crate::resource::{
    BufferDesc, BufferHandle, ImageDesc, ImageHandle, LogicalResource, LogicalResourceDescriptor,
    Res, ResourceUsage, Sampler, get_usage,
};

//mod caches;
pub mod pass;
pub mod resource;

type PassIndex = usize;
type ResIndex = usize;

// Derive already gives the right order
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct EdgeKey {
    start: usize,
    end: usize,
}

pub struct IndexedVec<Idx, Key> {
    vec: Vec<Key>,
    _marker: PhantomData<fn() -> Idx>,
}

impl<Idx, Key> IndexedVec<Idx, Key> {
    pub fn new() -> Self {
        Self {
            vec: vec![],
            _marker: PhantomData,
        }
    }
}

impl<Idx, Key> DerefMut for IndexedVec<Idx, Key> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.vec
    }
}

impl<Idx, Key> Deref for IndexedVec<Idx, Key> {
    type Target = Vec<Key>;

    fn deref(&self) -> &Self::Target {
        &self.vec
    }
}

struct Dependency {
    input_dependency: (),
    output_dependency: (),
}

type GraphEdges = BTreeMap<EdgeKey, Dependency>;

#[derive(Clone, Copy)]
pub(crate) struct BufferRef {
    buffer: rhi::GpuPtr,
}

#[derive(Clone, Copy)]
pub(crate) struct ImageRef {
    image: rhi::GpuPtr,
    properties: rhi::ImageInfo,
}

pub(crate) struct GraphResources {
    buffers: Vec<BufferRef>, // Res<BufferHandle>::id -> Buffer
    images: Vec<ImageRef>,   // Res<ImageHandle>::id -> Image
}

pub(crate) struct ImageHeap {}

impl GraphResources {
    pub fn new() -> Self {
        Self {
            buffers: vec![],
            images: vec![],
        }
    }

    pub fn new_buffer(&mut self, device: &mut rhi::Device, desc: &BufferDesc) -> BufferHandle {
        let buffer = device.create_buffer(&rhi::BufferInfo {
            memory: desc.location,
            size: desc.size as u64,
            usage: rhi::BufferUsage::General,
        });
        let idx = self.buffers.len();
        self.buffers.push(BufferRef { buffer });
        let device_addr = device.buffer_device_addr(buffer);
        unsafe { BufferHandle::new(idx, device_addr) }
    }

    pub fn new_image(&mut self, device: &mut rhi::Device, desc: &ImageDesc) -> ImageHandle {
        let image_info = rhi::ImageInfo {
            ty: todo!(),
            dimensions: todo!(),
            mip_count: todo!(),
            layer_count: todo!(),
            sample_count: todo!(),
            format: todo!(),
            usage: todo!(),
        };
        let image = device.create_image(&image_info);
        let idx = self.images.len();
        self.images.push(ImageRef {
            image,
            properties: image_info,
        });
        unsafe { ImageHandle::from_idx(idx) }
    }

    pub fn new_external_image(&mut self, image: rhi::GpuPtr) -> ImageHandle {
        let idx = self.images.len();
        // TODO: I need to differentiate between persistent resources, external resources, and transient resources
        self.images.push(ImageRef {
            image,
            properties: rhi::ImageInfo::default(),
        });
        unsafe { ImageHandle::from_idx(idx) }
    }
}

pub struct RenderGraph {
    device: rhi::Device,
    graphics_queue: rhi::Queue,
    swapchain: rhi::Swapchain,
    caches: Vec<GraphResources>,
    frame_index: u64,
}

pub struct Frame<'graph, 'frame> {
    graph: &'graph mut RenderGraph,
    passes: Vec<Box<dyn FnOnce(PassContext<'graph, '_>) + 'frame>>,
    //resources: &'graph mut GraphResources,
    res_handle_mapping: Vec<usize>, // Res<T>.id -> GraphResources.buffers|images
    res_pass_mapping: Vec<usize>,   // Res<T>.id -> Output Pass idx
    edges: GraphEdges,
    _frame: PhantomData<&'frame ()>,
}

pub struct PassBuilder<'graph, 'frame, 'pass> {
    frame: &'pass mut Frame<'graph, 'frame>,
}

pub struct PassContext<'graph, 'pass> {
    pub command_buffer: &'pass mut rhi::CommandBuffer,
    graph: &'pass mut RenderGraph,
    res_handle_mapping: &'pass Vec<usize>,
    _pass: PhantomData<&'graph ()>,
}

impl<'graph, 'frame, 'pass> PassBuilder<'graph, 'frame, 'pass> {
    pub fn input_ref<T: LogicalResource>(&mut self, id: Res<T>, usage: impl ResourceUsage) {
        let output = self.frame.res_pass_mapping[id.id];
        let input = self.frame.passes.len();
        if let Some(dependency) = self.frame.edges.get_mut(&EdgeKey {
            start: output,
            end: input,
        }) {
            // merge usages here
        } else {
            self.frame.edges.insert(
                EdgeKey {
                    start: output,
                    end: input,
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
        // Fetch the current pass index
        let pass = self.frame.passes.len();
        let usage = get_usage(usage);
        let resource = desc.to_resource(
            &mut self.frame.graph.device,
            &mut self.frame.graph.caches[self.frame.graph.frame_index as usize],
        );
        // TODO: need to forge the IDs here, rather than grabbing them
        // Outputted reference to resource gets a new ID
        let id = self.frame.res_pass_mapping.len();
        // Store the provenance pass index to res_pass_mapping so we can get the origin pass
        self.frame.res_pass_mapping.push(pass);
        self.frame.res_handle_mapping.push(resource.id());
        unsafe {
            let res = Res::from_idx(id);
            res
        }
    }

    pub fn build(self, callback: impl FnOnce(PassContext<'graph, '_>) + 'frame) {
        self.frame.passes.push(Box::new(callback));
    }
}

fn compile<Vertex, Edge>(nodes: &Vec<Vertex>, edges: &BTreeMap<EdgeKey, Edge>) -> Vec<usize> {
    // Step 1: count incoming edges, nodes without any are the starting nodes
    // A pass depending on itself is not an ordering constraint
    let mut in_degree = vec![0usize; nodes.len()];
    for edge in edges.keys().filter(|e| e.start != e.end) {
        in_degree[edge.end] += 1;
    }

    // Min-heap, so that ready nodes keep their declaration order
    let mut ready: Vec<usize> = in_degree
        .iter()
        .filter(|&&degree| degree == 0)
        .map(|degree| *degree)
        .collect();

    // Step 2: Reorder (Kahn's algorithm)
    let mut sorted = Vec::with_capacity(nodes.len());

    while let Some(node) = ready.pop() {
        sorted.push(node);
        // Edges are sorted by input pass, so this is every edge leaving `node`
        for key in edges
            .range(
                EdgeKey {
                    start: node,
                    end: 0,
                }..=EdgeKey {
                    start: node,
                    end: usize::MAX,
                },
            )
            .map(|(key, _)| key)
            .filter(|e| e.start != e.end)
        {
            in_degree[key.end] -= 1;
            if in_degree[key.end] == 0 {
                ready.push(key.end);
            }
        }
    }
    assert_eq!(
        sorted.len(),
        nodes.len(),
        "render graph contains a dependency cycle"
    );

    sorted
}

#[cfg(test)]
mod tests {

    fn indirect_sort<T>(arr: &mut [T], idxs: &mut [usize]) {
        let mut a = 0;
        for _ in 0..(arr.len() - 1) {
            let b = idxs[a];
            if a != b {
                arr.swap(a, b);
                idxs.swap(a, b);
            } else {
                a += 1;
            }
        }
    }

    #[test]
    fn test() {
        let mut arr = ['c', 'e', 'b', 'd', 'a', 'g', 'f'];

        indirect_sort(&mut arr, &mut [2, 4, 1, 3, 0, 6, 5]);

        assert_eq!(arr, ['a', 'b', 'c', 'd', 'e', 'f', 'g']);
    }
}

impl<'graph, 'frame> Frame<'graph, 'frame> {
    pub fn new_pass(&mut self, name: &str) -> PassBuilder<'graph, 'frame, '_> {
        PassBuilder { frame: self }
    }

    pub fn sampler(&mut self, info: &rhi::SamplerInfo) -> Sampler {
        let device = &mut self.graph.device;
        let descriptor = device.get_sampler_descriptor(info);
        // TODO: get samplers done
        unsafe { Sampler::from_idx(0) }
    }

    pub fn swapchain_image(&self) -> Res<ImageHandle> {
        todo!()
    }

    pub fn delete<T: LogicalResource>(&mut self, resource: Res<T>) {
        // Do nothing for now
        //todo!()
    }

    pub fn run(self) {
        let mut passes = self.passes;
        // Compile frame
        let compiled = compile(&passes, &self.edges);
        let mut a = 0;
        for _ in 0..compiled.len() {
            let b = compiled[a];
            passes.swap(a, b);
            a = b;
        }
        // block on completion of frame (n - FRAMES_IN_FLIGHT)

        // Grab new swapchain image

        // Perform passes
        for (idx, pass) in passes.into_iter().enumerate() {
            let mut command_buffer = self.graph.graphics_queue.begin_recording(0);
            pass(PassContext {
                command_buffer: &mut command_buffer,
                graph: self.graph,
                res_handle_mapping: &self.res_handle_mapping,
                _pass: PhantomData,
            });
            // crude but correct after topological sort
            command_buffer.barrier(rhi::Stage::AllCommands, rhi::Stage::AllCommands);
            unsafe {
                self.graph.graphics_queue.submit(&[command_buffer]).unwrap();
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
        let caches = (0..FRAMES_IN_FLIGHT)
            .into_iter()
            .map(|_| GraphResources::new())
            .collect();
        Self {
            device,
            graphics_queue,
            swapchain,
            caches,
            frame_index: 0,
        }
    }

    pub fn new_frame(&'graph mut self) -> Frame<'graph, 'frame> {
        let swapchain_image = self.swapchain.acquire_next_image().unwrap();
        let mut resources = &mut self.caches[self.frame_index as usize];
        let swapchain_handle = resources.new_external_image(swapchain_image);

        Frame {
            graph: self,
            passes: vec![],
            _frame: PhantomData,
            res_pass_mapping: vec![],
            res_handle_mapping: vec![],
            edges: BTreeMap::new(),
        }
    }

    pub fn device_mut(&mut self) -> &mut rhi::Device {
        &mut self.device
    }
}

pub trait Getter<T> {
    fn get(&self, resource: Res<T>) -> T;
}

impl Getter<BufferHandle> for PassContext<'_, '_> {
    fn get(&self, resource: Res<BufferHandle>) -> BufferHandle {
        let caches = &self.graph.caches[self.graph.frame_index as usize % self.graph.caches.len()];
        let handle = self.res_handle_mapping[resource.id];
        let ptr = caches.buffers[handle].buffer;
        todo!()
    }
}

impl Getter<ImageHandle> for PassContext<'_, '_> {
    fn get(&self, resource: Res<ImageHandle>) -> ImageHandle {
        let caches = &self.graph.caches[self.graph.frame_index as usize % self.graph.caches.len()];
        caches.images[resource.id];
        todo!()
    }
}

impl<'graph, 'pass> PassContext<'graph, 'pass> {
    //pub fn get<T: 'pass + LogicalResource>(&self, resource: Res<T>) -> T {
    //    let caches = &self.graph.caches[self.graph.frame_index as usize % self.graph.caches.len()];
    //    todo!()
    //}

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
