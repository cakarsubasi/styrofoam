use core::slice;
use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::CStr;
use std::mem::ManuallyDrop;
use std::ptr::null_mut;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::Weak;
use std::sync::atomic::AtomicI32;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use ash::VkResult;
use ash::ext;
//use ash::khr;
use ash::vk;
use ash::vk::TaggedStructure as _;
use raw_window_handle::RawDisplayHandle;
use raw_window_handle::RawWindowHandle;
use vk_mem::Alloc;

use super::command::LayoutTransition;

use super::command::PipelineType;
use super::instance::DescriptorHeapProps;
use super::instance::DeviceResult;

use super::instance::Instance;
use super::swapchain::Swapchain;

use crate::*;

impl Cull {
    fn to_vk(&self) -> (vk::CullModeFlags, vk::FrontFace) {
        match self {
            Cull::CCW => (vk::CullModeFlags::BACK, vk::FrontFace::COUNTER_CLOCKWISE),
            Cull::CW => (vk::CullModeFlags::BACK, vk::FrontFace::CLOCKWISE),
            Cull::BOTH => (vk::CullModeFlags::FRONT_AND_BACK, vk::FrontFace::CLOCKWISE),
            Cull::NONE => (vk::CullModeFlags::NONE, vk::FrontFace::CLOCKWISE),
        }
    }
}

#[repr(C)]
pub struct Semaphore {
    pub(super) device: Arc<DeviceHandles>,
    pub(super) inner: vk::Semaphore,
}

impl Semaphore {
    pub fn set_object_name(&self, name: &CStr) {
        self.device.set_object_name(self.inner, name);
    }
}

impl Drop for Semaphore {
    fn drop(&mut self) {
        unsafe {
            self.device.inner.destroy_semaphore(self.inner, None);
        }
    }
}

pub struct ShaderIR<'a> {
    pub bytes: &'a [u32],
    pub entry: &'a CStr,
}

pub(super) struct DeviceHandles {
    // Surface needs to be dropped before the instance, meaning if I move the surface out,
    // the instance needs to be behind a shared pointer so the device itself cannot drop it before the surface
    //pub surface: Surface,
    pub inner: ash::Device,
    pub instance: Instance,
    pub pdevice: vk::PhysicalDevice,
    pub allocator: ManuallyDrop<vk_mem::Allocator>,
    // Extensions
    pub debug_utils: ext::debug_utils::Device,
    pub descriptor_heap: ext::descriptor_heap::Device,
    pub descriptor_heap_props: DescriptorHeapProps,
    pub extended_dynamic_state3: ext::extended_dynamic_state3::Device,
    // pub device_address_commands: khr::device_address_commands::Device, // Poor support right now
}

impl DeviceHandles {
    pub(crate) fn set_object_name<T: ash::vk::Handle>(&self, handle: T, name: &CStr) {
        let debug_utils_object_name = vk::DebugUtilsObjectNameInfoEXT::default()
            .object_handle(handle)
            .object_name(name);
        unsafe {
            self.debug_utils
                .set_debug_utils_object_name(&debug_utils_object_name)
                .unwrap();
        }
    }
}

impl Drop for DeviceHandles {
    fn drop(&mut self) {
        unsafe {
            ManuallyDrop::drop(&mut self.allocator);
            self.inner.destroy_device(None);
        }
    }
}

struct CommandPool {
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    used: AtomicI32,
}

impl CommandPool {
    fn new(device: &ash::Device, queue_index: u32, command_buffer_count: u32) -> Self {
        unsafe {
            let command_pool_create_info = &vk::CommandPoolCreateInfo::default()
                //.flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(queue_index);

            let command_pool = device
                .create_command_pool(command_pool_create_info, None)
                .unwrap();

            let allocate_info = vk::CommandBufferAllocateInfo::default()
                .command_buffer_count(command_buffer_count)
                .command_pool(command_pool);

            let command_buffers = device.allocate_command_buffers(&allocate_info).unwrap();
            Self {
                command_pool,
                command_buffers,
                used: AtomicI32::new(0),
            }
        }
    }

    fn reset(&self, device: &ash::Device) {
        unsafe {
            device
                .reset_command_pool(
                    self.command_pool,
                    vk::CommandPoolResetFlags::RELEASE_RESOURCES,
                )
                .unwrap();

            self.used.store(0, Ordering::Release);
        }
    }

    unsafe fn destroy(&self, device: &ash::Device) {
        unsafe {
            device.destroy_command_pool(self.command_pool, None);
        }
    }
}

struct QueuePool {
    pub(crate) _family_index: u32,
    queue: vk::Queue,
    command_pools: Vec<CommandPool>,
}

impl QueuePool {
    fn new(
        device: &ash::Device,
        queue_index: u32,
        command_pools: u32,
        command_buffers_per_pool: u32,
    ) -> Self {
        unsafe {
            let queue = device.get_device_queue(queue_index, 0);

            let command_pools = (0..command_pools)
                .into_iter()
                .map(|_| CommandPool::new(device, queue_index, command_buffers_per_pool))
                .collect();

            Self {
                _family_index: queue_index,
                queue,
                command_pools: command_pools,
            }
        }
    }
}

pub struct Device {
    handles: Arc<DeviceHandles>,
    // Inner reference should be Weak maybe?
    heap: Arc<RwLock<DescriptorHeap>>,
}

impl Device {
    pub fn new() -> Self {
        todo!()
    }

    pub fn new_with_presentation(display_handle: RawDisplayHandle) -> Self {
        unsafe {
            let instance = Instance::new_with_presentation(display_handle);

            let DeviceResult {
                device,
                pdevice,
                graphics_queue_index: _,
                compute_queue_index: _,
                transfer_queue_index: _,
            } = instance.create_device();

            let mut allocator_create_info =
                vk_mem::AllocatorCreateInfo::new(&instance.instance, &device, pdevice);
            allocator_create_info.flags = vk_mem::AllocatorCreateFlags::BUFFER_DEVICE_ADDRESS;
            let allocator =
                vk_mem::Allocator::new(allocator_create_info).expect("Failed to create allocator");

            let debug_utils_loader = ext::debug_utils::Device::load(&instance.instance, &device);

            let descriptor_heap_props = instance.get_descriptor_heap_properties(&pdevice);

            let descriptor_heap_loader =
                ext::descriptor_heap::Device::load(&instance.instance, &device);

            let extended_dynamic_state3 =
                ext::extended_dynamic_state3::Device::load(&instance.instance, &device);

            let handles = Arc::new(DeviceHandles {
                instance,
                inner: device,
                pdevice,
                allocator: ManuallyDrop::new(allocator),
                debug_utils: debug_utils_loader,
                descriptor_heap: descriptor_heap_loader,
                descriptor_heap_props: descriptor_heap_props.unwrap(),
                extended_dynamic_state3,
            });

            let descriptor_heap = DescriptorHeap::new(Arc::clone(&handles)).unwrap();

            Self {
                handles: handles,
                heap: Arc::new(RwLock::new(descriptor_heap)),
            }
        }
    }

    fn device(&self) -> &ash::Device {
        &self.handles.inner
    }

    pub fn get_descriptor_heap_properties(&self) -> DescriptorHeapProps {
        self.handles.descriptor_heap_props.clone()
    }

    pub fn set_object_name(&self, obj: GpuPtr, name: &CStr) {
        let heap = self.heap.read().unwrap();

        if let Some(ref res) = heap.allocations.get(&obj.handle) {
            match res {
                HeapOwnedResource::Buffer(buffer) => {
                    self.handles.set_object_name(buffer.inner, name)
                }
                HeapOwnedResource::Image(image) => {
                    self.handles.set_object_name(image.inner, name);
                    if let Some(view) = image.view {
                        self.handles.set_object_name(view, name);
                    }
                }
            }
        }
    }
}

impl DeviceRHI for Device {
    type Pipeline = Pipeline;
    type Semaphore = Semaphore;
    type Queue = Queue;
    type GpuPtr = GpuPtr;

    fn create_buffer(&mut self, details: &BufferDesc) -> Self::GpuPtr {
        self.heap.write().unwrap().create_buffer(details)
    }

    fn create_image(&mut self, details: &ImageDesc) -> Self::GpuPtr {
        self.heap.write().unwrap().create_image(details)
    }

    fn buffer_host_ptr(&self, ptr: Self::GpuPtr) -> *mut u8 {
        let heap = self.heap.read().unwrap();

        let buffer = heap.ptr_to_buffer(ptr);

        // TODO: check alignment and size
        unsafe { buffer.mapped_ptr.unwrap().byte_add(ptr.offset as usize) }
    }

    fn buffer_device_ptr(&self, ptr: Self::GpuPtr) -> u64 {
        let heap = self.heap.read().unwrap();

        let buffer = heap.ptr_to_buffer(ptr);

        let info = vk::BufferDeviceAddressInfo::default().buffer(buffer.inner);
        unsafe {
            let addr = self.handles.inner.get_buffer_device_address(&info);
            // TODO: check alignment and size
            addr + (ptr.offset as u64)
        }
    }

    fn delete_ptr(&mut self, ptr: Self::GpuPtr) {
        self.heap.write().unwrap().free(ptr);
    }

    fn create_queue(
        &mut self,
        ty: QueueType,
        command_pools: u32,
        command_buffers_per_pool: u32,
    ) -> Self::Queue {
        match ty {
            QueueType::Graphics => Queue {
                device: Arc::downgrade(&self.handles),
                heap: Arc::downgrade(&self.heap),
                queue: QueuePool::new(&self.device(), 0, command_pools, command_buffers_per_pool),
                //swapchain: self.swapchain.as_ref().map(|s| Arc::downgrade(&s)),
            },
            //QueueType::Compute => todo!(),
            //QueueType::Copy => todo!(),
        }
    }

    fn create_semaphore(&mut self, initial_value: u64) -> Self::Semaphore {
        unsafe {
            let mut semaphore_info = vk::SemaphoreTypeCreateInfo::default()
                .semaphore_type(vk::SemaphoreType::TIMELINE)
                .initial_value(initial_value);

            let create_info = vk::SemaphoreCreateInfo::default().push(&mut semaphore_info);

            let semaphore = self.device().create_semaphore(&create_info, None).unwrap();

            Self::Semaphore {
                device: Arc::clone(&self.handles),
                inner: semaphore,
            }
        }
    }

    fn wait_semaphores(&self, semaphores: &[Self::Semaphore], values: &[u64]) {
        unsafe {
            assert_eq!(
                semaphores.len(),
                values.len(),
                "The length of semaphores and the waited values must match!"
            );
            let semaphores: Vec<_> = semaphores.iter().map(|s| s.inner).collect();

            let wait_info = &vk::SemaphoreWaitInfo::default()
                .semaphores(&semaphores)
                .values(&values);

            self.device().wait_semaphores(wait_info, u64::MAX).unwrap();
        }
    }

    fn create_compute_pipeline(&mut self, compute_ir: &ShaderIR) -> Self::Pipeline {
        let mut compute_shader =
            vk::ShaderModuleCreateInfo::default().code(bytemuck::cast_slice(compute_ir.bytes));

        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            //.module(compute_ir.module.shader_module)
            .name(&compute_ir.entry)
            .push(&mut compute_shader);

        let create_infos = [vk::ComputePipelineCreateInfo::default()
            .flags(vk::PipelineCreateFlags::empty())
            .stage(stage)
            .layout(vk::PipelineLayout::null())];

        unsafe {
            let pipelines = self
                .device()
                .create_compute_pipelines(vk::PipelineCache::null(), &create_infos, None)
                .unwrap();

            Self::Pipeline {
                device: Arc::clone(&self.handles),
                inner: pipelines[0],
                ty: PipelineType::Compute,
            }
        }
    }

    fn create_graphics_pipeline(
        &mut self,
        vertex_ir: &ShaderIR,
        fragment_ir: &ShaderIR,
        description: &RasterDescription,
    ) -> Self::Pipeline {
        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];

        let dynamic_state =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_attribute_descriptions(&[])
            .vertex_binding_descriptions(&[]);

        let mut rendering_create_info = vk::PipelineRenderingCreateInfo::default()
            .view_mask(0) // hmmmm
            .color_attachment_formats(description.color_formats)
            .depth_attachment_format(description.depth_format)
            .stencil_attachment_format(description.stencil_format);

        let (cull_mode, front_face) = description.cull.to_vk();
        let rasterization_state = vk::PipelineRasterizationStateCreateInfo::default()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(cull_mode)
            .front_face(front_face)
            .depth_bias_enable(false)
            .line_width(1.0);

        let multisample_state = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1) // probably should be configurable or use dynamic state
            .sample_shading_enable(false)
            .alpha_to_coverage_enable(description.alpha_to_coverage) // gotta learn multisample coverage
            .alpha_to_one_enable(false);

        let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(true)
            .depth_write_enable(true)
            .depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL)
            .stencil_test_enable(false)
            .min_depth_bounds(0.0)
            .max_depth_bounds(1.0);

        let color_blend_attachments = [vk::PipelineColorBlendAttachmentState::default()
            .blend_enable(false)
            .color_write_mask(vk::ColorComponentFlags::RGBA)];

        let color_blend_state = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .logic_op(vk::LogicOp::COPY)
            .attachments(&color_blend_attachments);

        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .scissor_count(1)
            .viewport_count(1);

        let mut vertex_shader =
            vk::ShaderModuleCreateInfo::default().code(bytemuck::cast_slice(vertex_ir.bytes));
        let mut fragment_shader =
            vk::ShaderModuleCreateInfo::default().code(bytemuck::cast_slice(fragment_ir.bytes));

        let pipeline_stages = [
            vk::PipelineShaderStageCreateInfo::default()
                //.module(vertex_ir.module.shader_module)
                .stage(vk::ShaderStageFlags::VERTEX)
                .name(&vertex_ir.entry)
                .push(&mut vertex_shader),
            vk::PipelineShaderStageCreateInfo::default()
                //.module(fragment_ir.module.shader_module)
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .name(&fragment_ir.entry)
                .push(&mut fragment_shader),
        ];

        let mut graphics_pipeline_flags = vk::PipelineCreateFlags2CreateInfo::default()
            .flags(vk::PipelineCreateFlags2::DESCRIPTOR_HEAP_EXT);

        let graphics_pipeline_create_info = vk::GraphicsPipelineCreateInfo::default()
            .flags(vk::PipelineCreateFlags::empty())
            .stages(&pipeline_stages)
            .vertex_input_state(&vertex_input_state)
            .input_assembly_state(&input_assembly_state)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterization_state)
            .multisample_state(&multisample_state)
            .depth_stencil_state(&depth_stencil_state)
            .color_blend_state(&color_blend_state)
            .dynamic_state(&dynamic_state)
            .layout(vk::PipelineLayout::null()) // VK_EXT_descriptor_heap
            .render_pass(vk::RenderPass::null()) // VK_KHR_dynamic_rendering
            .push(&mut rendering_create_info)
            .push(&mut graphics_pipeline_flags);

        unsafe {
            let pipelines = self
                .device()
                .create_graphics_pipelines(
                    vk::PipelineCache::null(), // todo: pipeline cache
                    &[graphics_pipeline_create_info],
                    None,
                )
                .map_err(|(_, res)| res)
                .unwrap();

            Pipeline {
                device: Arc::clone(&self.handles),
                inner: pipelines[0],
                ty: PipelineType::Graphics,
            }
        }
    }

    fn create_meshlet_pipeline(
        &mut self,
        _meshlet_ir: &ShaderIR,
        _fragment_ir: &ShaderIR,
        _description: &RasterDescription,
    ) -> Self::Pipeline {
        todo!()
    }

    fn get_image_descriptor(&self, image: Self::GpuPtr) -> ImageDescriptor {
        let heap = self.heap.read().unwrap();
        let mut descriptor = ImageDescriptor { inner: [0u64; 4] };
        heap.write_image_descriptor(image, &mut descriptor);
        descriptor
    }

    fn get_sampler_descriptor(&self, desc: &SamplerDesc) -> SamplerDescriptor {
        let heap = self.heap.read().unwrap();
        let mut descriptor = SamplerDescriptor { inner: [0u64; 4] };
        heap.write_sampler_descriptor(desc, &mut descriptor);
        descriptor
    }
}

impl SwapchainDeviceRHI for Device {
    type Swapchain = Swapchain;

    fn create_swapchain(
        &self,
        queue: &crate::Queue,
        window: &crate::WindowSystemData,
        info: &crate::SwapchainInfo,
    ) -> Self::Swapchain {
        unsafe {
            let handles = &self.handles;
            let heap = Arc::clone(&self.heap);
            let present_queue = queue.queue.queue;
            let swapchain =
                Swapchain::new(Arc::clone(&handles), heap, window, present_queue, info).unwrap();

            swapchain
        }
    }
}

pub struct Queue {
    device: Weak<DeviceHandles>,
    heap: Weak<RwLock<DescriptorHeap>>,
    queue: QueuePool,
}

impl Queue {
    fn get_command_buffer(&mut self, command_pool: u32) -> vk::CommandBuffer {
        let queue = &self.queue; //.upgrade().unwrap();
        let command_pool = queue
            .command_pools
            .get(command_pool as usize)
            .expect(&format!(
                "Invalid command pool index {}. There are only {} command pools available.",
                command_pool,
                queue.command_pools.len()
            ));

        if command_pool.used.load(Ordering::Acquire) == -1 {
            command_pool.reset(&self.device.upgrade().unwrap().inner);
        }

        let idx = command_pool.used.fetch_add(1, Ordering::Release);
        let command_buffer = command_pool.command_buffers.get(idx as usize).expect(&format!(
            "Attempted to requested {} command buffers. But this command pool only has {} available.",
            idx + 1,
            command_pool.command_buffers.len()));
        *command_buffer
    }

    fn submit_impl(
        &mut self,
        command_buffers: &[<Self as QueueRHI>::CommandBuffer],
    ) -> Result<(), Error> {
        unsafe {
            if command_buffers.is_empty() {
                return Ok(());
            }

            let device = self.device.upgrade().unwrap();
            let command_pool_idx = command_buffers
                .iter()
                .map(|cb| cb.command_pool_idx)
                .reduce(|acc, value| {
                    assert_eq!(
                        acc, value,
                        "Attempted to submit command buffers from different command pools",
                    );
                    value
                })
                .unwrap();
            for cb in command_buffers {
                // Good opportunity to handle device loss
                device.inner.end_command_buffer(cb.inner).unwrap();
            }

            let submit_info: Vec<_> = command_buffers
                .iter()
                .map(|cb| vk::CommandBufferSubmitInfo::default().command_buffer(cb.inner))
                .collect();

            let wait_semaphores: Vec<_> = command_buffers
                .iter()
                .flat_map(|cb| cb.wait.as_slice())
                .map(|info| {
                    vk::SemaphoreSubmitInfo::default()
                        .semaphore(info.semaphore)
                        .stage_mask(info.stage.into())
                        .value(info.value)
                })
                .collect();

            let signal_semaphores: Vec<_> = command_buffers
                .iter()
                .flat_map(|cb| cb.signal.as_slice())
                .map(|info| {
                    vk::SemaphoreSubmitInfo::default()
                        .semaphore(info.semaphore)
                        .stage_mask(info.stage.into())
                        .value(info.value)
                })
                .collect();
            let submits = [vk::SubmitInfo2::default()
                .command_buffer_infos(&submit_info)
                .signal_semaphore_infos(&signal_semaphores)
                .wait_semaphore_infos(&wait_semaphores)];
            // Good opportunity to handle device loss
            let queue = &mut self.queue; // .upgrade().unwrap();
            device
                .inner
                .queue_submit2(queue.queue, &submits, vk::Fence::null())
                .unwrap();

            // Store -1 to indicate command pool requires resetting
            queue.command_pools[command_pool_idx as usize]
                .used
                .store(-1, Ordering::Release);
        }
        Ok(())
    }
}

impl Drop for Queue {
    fn drop(&mut self) {
        let device = self.device.upgrade().unwrap();

        unsafe {
            device.inner.queue_wait_idle(self.queue.queue).unwrap();

            for command_pool in &self.queue.command_pools {
                command_pool.destroy(&device.inner)
            }
        }
    }
}

impl QueueRHI for Queue {
    type CommandBuffer = CommandBuffer;

    fn begin_recording(&mut self, command_pool: u32) -> Self::CommandBuffer {
        let command_buffer = self.get_command_buffer(command_pool);

        let begin_info = &vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        unsafe {
            // Good opportunity to handle device loss
            let device = self.device.upgrade().unwrap();
            device
                .inner
                .begin_command_buffer(command_buffer, begin_info)
                .unwrap();

            Self::CommandBuffer {
                device,
                heap: self.heap.upgrade().unwrap(),
                inner: command_buffer,
                command_pool_idx: command_pool,
                signal: vec![],
                wait: vec![],
                layout_transition_queue: vec![],
                presentation: None,
                render_pass_state: None,
            }
        }
    }

    fn submit(&mut self, command_buffers: &[Self::CommandBuffer]) -> Result<(), Error> {
        for cb in command_buffers {
            if let Some(swapchain_image) = cb.presentation {
                unsafe {
                    cb.multiple_layout_transition(&[LayoutTransition {
                        image: swapchain_image,
                        new_layout: vk::ImageLayout::PRESENT_SRC_KHR,
                        src_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                        src_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                        dst_stage_mask: vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
                        dst_access_mask: vk::AccessFlags2::empty(),
                    }]);
                }
            }
        }

        self.submit_impl(command_buffers)?;

        Ok(())
    }
}

// Might consider having two hash maps for either type and even splitting GpuPtr
pub(crate) enum HeapOwnedResource {
    Buffer(Buffer),
    Image(Image),
}

pub(super) struct DescriptorHeap {
    handle_counter: AtomicU64,
    allocations: HashMap<u64, HeapOwnedResource>,
    device: Arc<DeviceHandles>,
}

impl DescriptorHeap {
    pub fn new(device: Arc<DeviceHandles>) -> VkResult<Self> {
        eprintln!("heap props:\n{:?}", device.descriptor_heap_props);

        let resource_heap_size = device.descriptor_heap_props.max_resource_heap_size;
        let sampler_heap_size = device.descriptor_heap_props.max_sampler_heap_size;
        let image_descriptor_size = device.descriptor_heap_props.image_descriptor_size;
        let sampler_descriptor_size = device.descriptor_heap_props.sampler_descriptor_size;

        let maximum_images = (resource_heap_size
            - device
                .descriptor_heap_props
                .min_resource_heap_reserved_range)
            / image_descriptor_size;

        let maximum_samplers = (sampler_heap_size
            - device.descriptor_heap_props.min_sampler_heap_reserved_range)
            / sampler_descriptor_size;

        eprintln!("Maximum images: {}", maximum_images);
        eprintln!("Maximum samplers: {}", maximum_samplers);

        Ok(Self {
            handle_counter: AtomicU64::new(1),
            device,
            allocations: HashMap::new(),
        })
    }

    fn write_sampler_descriptor(&self, desc: &SamplerDesc, addr: &mut SamplerDescriptor) {
        let device = &self.device;
        let descriptor_heap = &device.descriptor_heap;

        unsafe {
            let descriptor = [vk::HostAddressRangeEXT::default()
                .address(bytemuck::cast_slice_mut(&mut addr.inner))];

            let anisotropy_enable = desc.anisotropy > 1.0;
            let max_anisotropy = if anisotropy_enable {
                desc.anisotropy
            } else {
                0.0
            };
            let compare_enable = desc.compare_op.is_some();
            let compare_op = if let Some(compare_op) = desc.compare_op {
                compare_op
            } else {
                CompareOp::Never // doesn't matter
            };

            let samplers = [
                vk::SamplerCreateInfo::default()
                    //.flags(vk::SamplerCreateFlags::DESCRIPTOR_BUFFER_CAPTURE_REPLAY_EXT)
                    .mag_filter(desc.mag_filter.into())
                    .min_filter(desc.min_filter.into())
                    .mipmap_mode(desc.mipmap_mode.into())
                    .address_mode_u(desc.address_mode[0].into())
                    .address_mode_v(desc.address_mode[1].into())
                    .address_mode_w(desc.address_mode[2].into())
                    .anisotropy_enable(anisotropy_enable)
                    .max_anisotropy(max_anisotropy)
                    .mip_lod_bias(desc.lod_bias)
                    .min_lod(desc.lod_range[0])
                    .max_lod(desc.lod_range[1])
                    .compare_enable(compare_enable)
                    .compare_op(compare_op.into())
                    .border_color(vk::BorderColor::FLOAT_OPAQUE_WHITE)
                    .unnormalized_coordinates(false), // don't support
            ];

            descriptor_heap
                .write_sampler_descriptors(&samplers, &descriptor)
                .unwrap();
        }
    }

    fn write_image_descriptor(&self, image: GpuPtr, addr: &mut ImageDescriptor) {
        let image = self.ptr_to_image(image);

        let device = &self.device;
        let descriptor_heap = &device.descriptor_heap;
        let _props = &device.descriptor_heap_props;
        unsafe {
            let resource = [vk::ResourceDescriptorInfoEXT::default()
                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                .data(vk::ResourceDescriptorDataEXT {
                    p_image: &vk::ImageDescriptorInfoEXT {
                        p_view: &vk::ImageViewCreateInfo::default()
                            .view_type(image.desc.ty.into())
                            .format(image.desc.format)
                            .image(image.inner)
                            .subresource_range(vk::ImageSubresourceRange {
                                aspect_mask: vk::ImageAspectFlags::COLOR,
                                base_mip_level: 0,
                                level_count: image.desc.mip_count,
                                base_array_layer: 0,
                                layer_count: image.desc.layer_count,
                            }),
                        layout: vk::ImageLayout::GENERAL,
                        ..Default::default()
                    },
                })];
            let descriptor = [vk::HostAddressRangeEXT::default()
                .address(bytemuck::cast_slice_mut(&mut addr.inner))];
            descriptor_heap
                .write_resource_descriptors(&resource, &descriptor)
                .unwrap();
        }
    }

    pub fn ptr_to_buffer(&self, ptr: GpuPtr) -> &Buffer {
        match self.allocations.get(&ptr.handle) {
            Some(HeapOwnedResource::Buffer(buffer)) => buffer,
            _ => panic!(),
        }
    }

    pub fn ptr_to_image(&self, ptr: GpuPtr) -> &Image {
        match self.allocations.get(&ptr.handle) {
            Some(HeapOwnedResource::Image(image)) => image,
            _ => panic!(),
        }
    }

    fn create_buffer(&mut self, desc: &BufferDesc) -> GpuPtr {
        let buffer = Buffer::new(Arc::clone(&self.device), desc).unwrap();

        let handle = self.handle_counter.fetch_add(1, Ordering::Relaxed);

        let size = buffer.size as u32;
        self.allocations
            .insert(handle, HeapOwnedResource::Buffer(buffer));

        GpuPtr {
            handle,
            offset: 0,
            size,
        }
    }

    fn create_image(&mut self, desc: &ImageDesc) -> GpuPtr {
        // TODO: use format
        let image = Image::new(Arc::clone(&self.device), desc);

        let handle = self.handle_counter.fetch_add(1, Ordering::Relaxed);

        let size = image.len() as u32;
        self.allocations
            .insert(handle, HeapOwnedResource::Image(image));

        GpuPtr {
            handle,
            offset: 0,
            size,
        }
    }

    fn free(&mut self, ptr: GpuPtr) {
        let res = self.allocations.remove(&ptr.handle);

        if let None = res {
            panic!("Double free.");
        } else if let Some(HeapOwnedResource::Image(image)) = res {
            if image.is_swapchain_image() {
                panic!("Swapchain images should not be passed to delete_ptr()");
            } else {
                image.free_resources(&self.device);
            }
        } else if let Some(HeapOwnedResource::Buffer(buffer)) = res {
            buffer.free_resources(&self.device);
        }
    }

    pub fn free_infallible(&mut self, ptr: GpuPtr) {
        if ptr.is_null() {
            return;
        }

        match self.allocations.remove(&ptr.handle) {
            Some(HeapOwnedResource::Buffer(buffer)) => buffer.free_resources(&self.device),
            Some(HeapOwnedResource::Image(image)) => image.free_resources(&self.device),
            None => {}
        }
    }

    pub fn insert_swapchain_image(&mut self, image: Image) -> GpuPtr {
        let handle = self.handle_counter.fetch_add(1, Ordering::Relaxed);

        let size = image.len() as u32;
        self.allocations
            .insert(handle, HeapOwnedResource::Image(image));

        GpuPtr {
            handle,
            offset: 0,
            size,
        }
    }
}

impl Drop for DescriptorHeap {
    fn drop(&mut self) {
        unsafe {
            self.device.inner.device_wait_idle().unwrap();

            for (_, res) in self.allocations.drain() {
                match res {
                    HeapOwnedResource::Buffer(buffer) => buffer.free_resources(&self.device),
                    HeapOwnedResource::Image(image) => image.free_resources(&self.device),
                }
            }
        }
    }
}

impl GpuPtr {
    pub fn null() -> Self {
        Self {
            handle: 0,
            offset: 0,
            size: 0,
        }
    }

    pub fn is_null(&self) -> bool {
        self.handle == 0
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct GpuPtr {
    handle: u64,
    pub offset: u32,
    pub size: u32,
}

impl BufferUsage {
    fn usage(&self) -> vk::BufferUsageFlags {
        match self {
            BufferUsage::DescriptorHeap => vk::BufferUsageFlags::DESCRIPTOR_HEAP_EXT,
            _ => {
                vk::BufferUsageFlags::INDEX_BUFFER
                    | vk::BufferUsageFlags::INDIRECT_BUFFER
                    | vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::UNIFORM_BUFFER
            }
        }
    }
}

impl Memory {
    fn vma_options(&self) -> vk_mem::AllocationCreateInfo {
        match self {
            Memory::Default => vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::Auto,
                flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
                ..Default::default()
            },
            Memory::DeviceOnly => vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::AutoPreferDevice,
                ..Default::default()
            },
            Memory::HostCoherent => vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::Auto,
                flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_RANDOM,
                required_flags: vk::MemoryPropertyFlags::HOST_COHERENT,
                ..Default::default()
            },
        }
    }
}

pub(super) struct Buffer {
    pub inner: vk::Buffer,
    allocation: vk_mem::Allocation,
    size: u64,
    pub _ty: BufferUsage,
    mapped_ptr: Option<*mut u8>,
}

impl Buffer {
    pub fn new(device: Arc<DeviceHandles>, desc: &BufferDesc) -> VkResult<Self> {
        unsafe {
            let size = desc.size;
            let buffer_usage = desc.usage;

            let buffer_info = vk::BufferCreateInfo::default()
                // HMMMM
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .size(size)
                .usage(
                    buffer_usage.usage()
                        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                        | vk::BufferUsageFlags::TRANSFER_SRC
                        | vk::BufferUsageFlags::TRANSFER_DST,
                );

            let allocation_info = desc.memory.vma_options();

            let (buffer, mut allocation) = device
                .allocator
                .create_buffer(&buffer_info, &allocation_info)?;

            let mapped_ptr = if let Memory::DeviceOnly = desc.memory {
                None
            } else {
                Some(device.allocator.map_memory(&mut allocation).unwrap())
            };

            Ok(Self {
                inner: buffer,
                allocation,
                size: size,
                _ty: buffer_usage,
                mapped_ptr,
            })
        }
    }

    //#[allow(unused)]
    //pub fn copy_to_buffer(&self, data: &[u8], dst_offset: u64) {
    //    if data
    //        .len()
    //        .checked_add(dst_offset as usize)
    //        .expect("Buffer offset overflow")
    //        > self.len() as usize
    //    {
    //        panic!("")
    //    }
    //    unsafe {
    //        // This is safe with &self because VMA uses an internal mutex
    //        self.device
    //            .allocator
    //            .copy_memory_to_allocation(&self.allocation, data, dst_offset)
    //            .unwrap();
    //    }
    //}

    //#[allow(unused)]
    //pub fn with_mapping(&mut self, f: impl FnOnce(&mut [u8])) {
    //    // Safety: &mut self is required because calling any buffer function inside
    //    // f would create aliasing &mut
    //    unsafe {
    //        let size = self.len();
    //        let mapping = self
    //            .device
    //            .allocator
    //            .map_memory(&mut self.allocation)
    //            .unwrap();

    //        let mapping = slice::from_raw_parts_mut(mapping, size as usize);
    //        f(mapping);

    //        self.device.allocator.unmap_memory(&mut self.allocation);
    //    }
    //}

    pub fn len(&self) -> u64 {
        self.size
    }

    #[allow(unused)]
    fn device_address(&self, device: &DeviceHandles) -> vk::DeviceAddress {
        unsafe {
            let address = device.inner.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(self.inner),
            );

            address
        }
    }

    #[allow(unused)]
    pub fn device_address_range(&self, device: &DeviceHandles) -> vk::DeviceAddressRangeKHR {
        let address = self.device_address(device);
        let size = self.len();
        vk::DeviceAddressRangeKHR { address, size }
    }

    pub fn free_resources(mut self, device: &DeviceHandles) {
        unsafe {
            if let Some(_) = self.mapped_ptr {
                device.allocator.unmap_memory(&mut self.allocation);
            }

            device
                .allocator
                .destroy_buffer(self.inner, &mut self.allocation);
        }
    }
}

impl ImageDesc {
    fn sample_count(&self) -> vk::SampleCountFlags {
        match self.sample_count {
            1 => vk::SampleCountFlags::TYPE_1,
            2 => vk::SampleCountFlags::TYPE_2,
            4 => vk::SampleCountFlags::TYPE_4,
            8 => vk::SampleCountFlags::TYPE_8,
            x => panic!("Invalid sample count {x}"),
        }
    }
}

fn image_type_to_image_view_type(ty: vk::ImageType) -> vk::ImageViewType {
    match ty {
        vk::ImageType::TYPE_1D => vk::ImageViewType::TYPE_1D,
        vk::ImageType::TYPE_2D => vk::ImageViewType::TYPE_2D,
        vk::ImageType::TYPE_3D => vk::ImageViewType::TYPE_3D,
        _ => unreachable!(),
    }
}

impl ImageUsage {
    fn to_vk(&self) -> vk::ImageUsageFlags {
        return vk::ImageUsageFlags::INPUT_ATTACHMENT
            | vk::ImageUsageFlags::COLOR_ATTACHMENT
            | vk::ImageUsageFlags::STORAGE
            | vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::TRANSFER_DST;

        match self {
            ImageUsage::Sampled => {
                vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_SRC
                    | vk::ImageUsageFlags::TRANSFER_DST
            }
            ImageUsage::Storage => {
                vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::TRANSFER_SRC
                    | vk::ImageUsageFlags::TRANSFER_DST
            }
            ImageUsage::Attachment => {
                vk::ImageUsageFlags::INPUT_ATTACHMENT
                    | vk::ImageUsageFlags::TRANSFER_SRC
                    | vk::ImageUsageFlags::TRANSFER_DST
            }
        }
    }
}

pub(crate) struct AllocatedImageData {
    pub size: usize,
    pub allocation: vk_mem::Allocation,
}

pub(crate) struct SwapchainImageData {
    pub idx: u32,
    pub submit_wait: Cell<vk::Semaphore>,
    pub submit_signal_present_wait: Cell<vk::Semaphore>,
}

pub(crate) enum ImageData {
    Allocated(AllocatedImageData),
    Swapchain(SwapchainImageData),
}

pub(crate) struct Image {
    pub inner: vk::Image,
    pub view: Option<vk::ImageView>,
    pub desc: ImageDesc,
    pub current_layout: Cell<vk::ImageLayout>,
    pub data: ImageData,
}

impl Image {
    // Create a 2D Image with the given extent
    pub fn new(device: Arc<DeviceHandles>, description: &ImageDesc) -> Self {
        unsafe {
            let layout = vk::ImageLayout::UNDEFINED;
            let image_info = vk::ImageCreateInfo::default()
                //.flags()
                .image_type(description.ty.into())
                .format(description.format)
                .extent(vk::Extent3D {
                    width: description.dimensions[0],
                    height: description.dimensions[1],
                    depth: description.dimensions[2],
                })
                .mip_levels(description.mip_count)
                .array_layers(description.layer_count)
                .samples(description.sample_count())
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(description.usage.to_vk())
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(layout)
            //.initial_layout(vk::ImageLayout::UNDEFINED);
                ;

            let allocation_info = vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::Auto,
                //flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
                ..Default::default()
            };

            let (image, allocation) = device
                .allocator
                .create_image(&image_info, &allocation_info)
                .unwrap();

            let memory_req = device.inner.get_image_memory_requirements(image);

            let view = match description.usage {
                _ => Some(
                    device
                        .inner
                        .create_image_view(
                            &vk::ImageViewCreateInfo::default()
                                .view_type(description.ty.into())
                                .format(description.format)
                                .image(image)
                                .subresource_range(vk::ImageSubresourceRange {
                                    aspect_mask: vk::ImageAspectFlags::COLOR, // TODO: add multiple attachment types or try to infer the attachment type
                                    base_mip_level: 0,
                                    level_count: description.mip_count,
                                    base_array_layer: 0,
                                    layer_count: description.layer_count,
                                }),
                            None,
                        )
                        .unwrap(),
                ),
                _ => None,
            };

            let data = AllocatedImageData {
                //device,
                size: memory_req.size as usize,
                allocation,
            };

            Self {
                inner: image,
                view,
                desc: description.clone(),
                current_layout: Cell::new(layout),
                data: ImageData::Allocated(data),
            }
        }
    }

    pub fn len(&self) -> usize {
        match &self.data {
            ImageData::Allocated(allocated_image_data) => allocated_image_data.size,
            ImageData::Swapchain(swapchain_image_data) => 9999999999, // need to figure this out
        }
    }

    pub fn is_swapchain_image(&self) -> bool {
        matches!(self.data, ImageData::Swapchain(_))
    }

    pub fn extent2d(&self) -> vk::Extent2D {
        vk::Extent2D {
            width: self.desc.dimensions[0],
            height: self.desc.dimensions[1],
        }
    }

    pub fn extent3d(&self) -> vk::Extent3D {
        vk::Extent3D {
            width: self.desc.dimensions[0],
            height: self.desc.dimensions[1],
            depth: self.desc.dimensions[2],
        }
    }

    pub fn extent_is_within_bounds(&self, offset: vk::Offset3D, extent: vk::Extent3D) -> bool {
        let extent_of_this = self.extent3d();

        (offset.x as u32 + extent.width) <= extent_of_this.width
            && (offset.y as u32 + extent.height) <= extent_of_this.height
            && (offset.z as u32 + extent.depth) <= extent_of_this.depth
    }

    pub fn offset_is_within_bounds(&self, offset: vk::Offset3D) -> bool {
        let extent_of_this = self.extent3d();

        offset.x <= extent_of_this.width as i32
            && offset.y <= extent_of_this.height as i32
            && offset.z <= extent_of_this.depth as i32
    }

    pub fn free_resources(mut self, device: &DeviceHandles) {
        unsafe {
            if let Some(view) = self.view {
                device.inner.destroy_image_view(view, None);
            }

            if let ImageData::Allocated(data) = &mut self.data {
                device
                    .allocator
                    .destroy_image(self.inner, &mut data.allocation);
            }
        }
    }
}
