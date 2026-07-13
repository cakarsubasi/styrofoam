use core::slice;
use std::cell::Cell;
use std::ffi::CStr;
use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::Weak;
use std::sync::atomic::AtomicI32;
use std::sync::atomic::Ordering;

use ash::VkResult;
use ash::ext;
use ash::khr;
use ash::vk;
use ash::vk::TaggedStructure as _;
use vk_mem::Alloc;
use winit::raw_window_handle::RawDisplayHandle;
use winit::raw_window_handle::RawWindowHandle;

use crate::renderer::vulkan::command::PipelineType;
use crate::renderer::vulkan::command::PresentSubmitEtc;
use crate::renderer::vulkan::command::SemaphoreInfo;
use crate::renderer::vulkan::instance::DescriptorHeapProps;
use crate::renderer::vulkan::instance::DeviceResult;

use super::*;

// We will cache stuff we check in instance creation and then use it as needed
pub struct DeviceExtensions {
    pub(super) descriptor_heap: Option<ExtDescriptorHeap>,
    pub(super) extended_dynamic_state3: Option<ExtExtendedDynamicState3>,
}

pub struct ExtExtendedDynamicState3 {
    pub(super) device: ext::extended_dynamic_state3::Device,
}

pub struct ExtDescriptorHeap {
    pub(super) device: ext::descriptor_heap::Device,
    pub(super) props: DescriptorHeapProps,
}

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

#[repr(transparent)]
pub struct TimelineSemaphore {
    pub(super) inner: vk::Semaphore,
}

impl SemaphoreRHI for TimelineSemaphore {
    fn wait(&mut self, value: u64) {
        todo!()
    }
}

pub struct ShaderIR2<'a> {
    pub bytes: &'a [u8],
    pub entry: &'a CStr,
}

pub struct DeviceHandles {
    pub instance: Instance,
    pub surface: Surface,
    pub inner: ash::Device,
    pub pdevice: vk::PhysicalDevice,
    pub allocator: ManuallyDrop<vk_mem::Allocator>,
    pub debug_utils: ext::debug_utils::Device,
    pub descriptor_heap: ext::descriptor_heap::Device,
    pub descriptor_heap_props: DescriptorHeapProps,
    pub extended_dynamic_state3: ext::extended_dynamic_state3::Device,
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

    fn destroy(self, device: &ash::Device) {
        unsafe {
            device.destroy_command_pool(self.command_pool, None);
        }
    }
}

struct OwnedQueue {
    family_index: u32,
    queues: Vec<vk::Queue>,
    queues_used: usize,
    command_pools: Vec<CommandPool>,
}

#[derive(Clone, Copy)]
struct QueueInfo {
    queue: vk::Queue,
    family: u32,
}

impl OwnedQueue {
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
                family_index: queue_index,
                queues: vec![queue],
                queues_used: 0,
                command_pools: command_pools,
            }
        }
    }
}

pub struct Device2 {
    handles: Arc<DeviceHandles>,
    // Inner reference should be Weak maybe?
    heap: Arc<RwLock<DescriptorHeap>>,
    swapchain: Option<Arc<Swapchain>>,
}

impl Device2 {
    pub fn new_with_presentation(
        display_handle: RawDisplayHandle,
        window_handle: RawWindowHandle,
    ) -> Self {
        unsafe {
            let instance = Instance::new_with_presentation(display_handle);
            let surface = instance.create_surface(display_handle, window_handle);

            let DeviceResult {
                device,
                pdevice,
                graphics_queue_index,
                compute_queue_index,
                transfer_queue_index,
            } = instance.create_device(&surface);

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
                surface,
                inner: device,
                pdevice,
                allocator: ManuallyDrop::new(allocator),
                debug_utils: debug_utils_loader,
                descriptor_heap: descriptor_heap_loader,
                descriptor_heap_props: descriptor_heap_props.unwrap(),
                extended_dynamic_state3,
            });

            let descriptor_heap = DescriptorHeap::new(Arc::clone(&handles)).unwrap();

            let swapchain = Swapchain::new(Arc::clone(&handles)).unwrap();
            Self {
                handles: handles,
                heap: Arc::new(RwLock::new(descriptor_heap)),
                swapchain: Some(Arc::new(swapchain)),
            }
        }
    }

    fn device(&self) -> &ash::Device {
        &self.handles.inner
    }
}

impl DeviceRHI for Device2 {
    //type ShaderText = ShaderIR2<'a>;

    type Pipeline = super::Pipeline;

    type Semaphore = TimelineSemaphore;

    type Queue = QueueRef;

    type GpuPtr = GpuPtr;

    fn create_buffer(&mut self, details: &BufferDesc) -> Self::GpuPtr {
        self.heap.write().unwrap().create_buffer(details)
    }

    fn create_image(&mut self, details: &ImageDesc) -> Self::GpuPtr {
        self.heap.write().unwrap().create_image(details)
    }

    fn with_mapping(&mut self, ptr: Self::GpuPtr, f: fn(&mut [u8])) {
        self.heap.write().unwrap().with_mapping(ptr, f);
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
            QueueType::Graphics => QueueRef {
                device: Arc::downgrade(&self.handles),
                heap: Arc::downgrade(&self.heap),
                idx: 0,
                queue: OwnedQueue::new(&self.device(), 0, command_pools, command_buffers_per_pool),
                swapchain: self.swapchain.as_ref().map(|s| Arc::downgrade(&s)),
                frames: 0,
            },
            QueueType::Compute => todo!(),
            QueueType::Copy => todo!(),
        }
    }

    fn create_semaphore(&mut self, initial_value: u64) -> Self::Semaphore {
        unsafe {
            let mut semaphore_info = vk::SemaphoreTypeCreateInfo::default()
                .semaphore_type(vk::SemaphoreType::TIMELINE)
                .initial_value(initial_value);

            let create_info = vk::SemaphoreCreateInfo::default().push(&mut semaphore_info);

            let semaphore = self.device().create_semaphore(&create_info, None).unwrap();

            // TODO: semaphore destruction
            Self::Semaphore { inner: semaphore }
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

    fn create_compute_pipeline(&mut self, compute_ir: &ShaderIR2) -> Self::Pipeline {
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
        vertex_ir: &ShaderIR2,
        fragment_ir: &ShaderIR2,
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
        meshlet_ir: &ShaderIR2,
        fragment_ir: &ShaderIR2,
        description: &RasterDescription,
    ) -> Self::Pipeline {
        todo!()
    }
}

pub struct QueueRef {
    device: Weak<DeviceHandles>,
    heap: Weak<RwLock<DescriptorHeap>>,
    idx: u32,
    queue: OwnedQueue,
    swapchain: Option<Weak<Swapchain>>,
    frames: u64,
}

impl QueueRef {
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

    pub fn begin_recording_presentation(
        &mut self,
        command_pool: u32,
        frame_index: u64,
    ) -> <Self as QueueRHI>::CommandBuffer {
        let next_frame = self
            .swapchain
            .as_ref()
            .unwrap()
            .upgrade()
            .unwrap()
            .next_frame(frame_index)
            .unwrap();

        let mut command_buffer = self.begin_recording(command_pool);
        command_buffer.signal.push(SemaphoreInfo {
            semaphore: next_frame.submit_signal_present_wait,
            value: 0,
            stage: vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
        });
        command_buffer.wait.push(SemaphoreInfo {
            semaphore: next_frame.submit_wait,
            value: 0,
            stage: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        });
        command_buffer.present = Some(PresentSubmitEtc {
            image_idx: next_frame.image_idx,
            semaphore: next_frame.submit_signal_present_wait,
            swapchain_extent: next_frame.image.extent,
        });
        command_buffer
    }

    pub fn submit_and_present(&mut self, command_buffer: &<Self as QueueRHI>::CommandBuffer) {
        let swapchain = self.swapchain.as_ref().unwrap().upgrade().unwrap();

        self.submit(slice::from_ref(command_buffer));
        let queue = self.queue.queues[self.idx as usize]; //.upgrade().unwrap().queues[self.idx as usize];

        let frame = command_buffer.present.as_ref().unwrap();

        let swapchains = [swapchain.swapchain];
        let wait_semaphores = [frame.semaphore];
        let indices = [frame.image_idx];
        let present_info = vk::PresentInfoKHR::default()
            .swapchains(&swapchains)
            .wait_semaphores(&wait_semaphores)
            .image_indices(&indices);
        unsafe {
            swapchain
                .swapchain_loader
                .queue_present(queue, &present_info)
                .unwrap(); // Must handle out of date
        }
    }
}

impl QueueRHI for QueueRef {
    type CommandBuffer = super::CommandBuffer;

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
                present: None,
            }
        }
    }

    fn submit(&mut self, command_buffers: &[Self::CommandBuffer]) {
        unsafe {
            if command_buffers.is_empty() {
                return;
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
                        .stage_mask(info.stage)
                        .value(info.value)
                })
                .collect();

            let signal_semaphores: Vec<_> = command_buffers
                .iter()
                .flat_map(|cb| cb.signal.as_slice())
                .map(|info| {
                    vk::SemaphoreSubmitInfo::default()
                        .semaphore(info.semaphore)
                        .stage_mask(info.stage)
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
                .queue_submit2(queue.queues[self.idx as usize], &submits, vk::Fence::null())
                .unwrap();

            // Store -1 to indicate command pool requires resetting
            queue.command_pools[command_pool_idx as usize]
                .used
                .store(-1, Ordering::Release);
        }
    }
}

enum HeapOwnedResource {
    Buffer(Buffer),
    Image(Image),
    Empty,
}

pub struct DescriptorHeap {
    device: Arc<DeviceHandles>,
    resource_heap: Buffer,
    sampler_heap: Buffer,
    heap_info: Vec<HeapOwnedResource>,
    dirty: Vec<u32>,
    free_list: Vec<u32>,
    next_free: u32,
}

impl DescriptorHeap {
    pub fn new(device: Arc<DeviceHandles>) -> VkResult<Self> {
        eprintln!("heap props:\n{:?}", device.descriptor_heap_props);

        let descriptor_heap = &device.descriptor_heap;
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

        let resource_heap = Buffer::new(
            Arc::clone(&device),
            &BufferDesc {
                memory: Memory::Default,
                size: resource_heap_size,
                usage: BufferUsage::DescriptorHeap,
            },
        )?;
        let sampler_heap = Buffer::new(
            Arc::clone(&device),
            &BufferDesc {
                memory: Memory::Default,
                size: sampler_heap_size,
                usage: BufferUsage::DescriptorHeap,
            },
        )?;

        let heap_info = (0..maximum_images)
            .into_iter()
            .map(|_| HeapOwnedResource::Empty)
            .collect();

        Ok(Self {
            device,
            resource_heap,
            sampler_heap,
            heap_info,
            dirty: vec![],
            free_list: vec![],
            next_free: 0,
        })
    }

    // Hmmm, this binding can be performed one time per our command buffer
    pub unsafe fn bind(&self, command_buffer: vk::CommandBuffer) {
        unsafe {
            let device = &self.resource_heap.device;
            let descriptor_heap = &device.descriptor_heap;
            let props = &device.descriptor_heap_props;

            let resource_addr = device.inner.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(self.resource_heap.inner),
            );

            let resource_bind_info = vk::BindHeapInfoEXT::default()
                .heap_range(
                    vk::DeviceAddressRangeKHR::default()
                        .address(resource_addr)
                        .size(self.resource_heap.len()),
                )
                .reserved_range_offset(
                    props.max_resource_heap_size - props.min_resource_heap_reserved_range,
                )
                .reserved_range_size(props.min_resource_heap_reserved_range);

            descriptor_heap.cmd_bind_resource_heap(command_buffer, &resource_bind_info);

            let barriers = [vk::MemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::HOST)
                .src_access_mask(vk::AccessFlags2::HOST_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::ALL_GRAPHICS)
                .dst_access_mask(
                    vk::AccessFlags2::RESOURCE_HEAP_READ_EXT
                        | vk::AccessFlags2::SAMPLER_HEAP_READ_EXT,
                )];

            device.inner.cmd_pipeline_barrier2(
                command_buffer,
                &vk::DependencyInfo::default().memory_barriers(&barriers),
            );
        }
    }

    fn get_free_idx(&mut self) -> u32 {
        if !self.free_list.is_empty() {
            self.free_list.pop().unwrap()
        } else {
            let free_idx = self.next_free;
            self.next_free += 1;
            free_idx
        }
    }

    fn update_heap(&mut self) {
        let device = &self.device;
        let descriptor_heap = &device.descriptor_heap;
        let props = &device.descriptor_heap_props;

        let resources: Vec<_> =
            self.dirty
                .iter()
                .map(|&idx| &self.heap_info[idx as usize])
                .map(|desc| match desc {
                    HeapOwnedResource::Buffer(buffer) => vk::ResourceDescriptorInfoEXT::default()
                        .data(vk::ResourceDescriptorDataEXT {
                            p_address_range: &unsafe { buffer.device_address_range() },
                        })
                        .ty(buffer.ty.descriptor_type()),
                    HeapOwnedResource::Image(image) => vk::ResourceDescriptorInfoEXT::default()
                        .data(vk::ResourceDescriptorDataEXT {
                            p_image: &unsafe { vk::ImageDescriptorInfoEXT::default() }, // TODO
                        }),
                    HeapOwnedResource::Empty => panic!(),
                })
                .collect();

        let desc_size = props.buffer_descriptor_size as usize;

        self.resource_heap.with_mapping(|addr| unsafe {
            let addr = addr.as_mut_ptr();
            let descriptors: Vec<_> = self
                .dirty
                .iter()
                .map(|&idx| vk::HostAddressRangeEXT {
                    address: (addr.byte_add(idx as usize * desc_size)) as *mut c_void,
                    size: desc_size,
                    ..Default::default()
                })
                .collect();

            descriptor_heap
                .write_resource_descriptors(&resources, &descriptors)
                .unwrap();
        });
    }

    pub fn ptr_to_buffer(&self, ptr: GpuPtr) -> &Buffer {
        match self.heap_info[ptr.inner as usize] {
            HeapOwnedResource::Buffer(ref buffer) => buffer,
            _ => panic!(),
        }
    }

    pub fn ptr_to_image(&self, ptr: GpuPtr) -> &Image {
        match self.heap_info[ptr.inner as usize] {
            HeapOwnedResource::Image(ref image) => image,
            _ => panic!("Given pointer does not point to an image"),
        }
    }
}

#[derive(Clone, Copy)]
enum GpuPtr2 {
    Handle(u32, u32), // u32 -> Buffer | (u32, u32) -> Image
    Ptr(u64),         // u64 -> DeviceAddress
    Swapchain(u32),   // u32 -> SwapchainImage
}

impl GpuPtr2 {
    pub(crate) fn null() -> Self {
        GpuPtr2::Ptr(u64::MAX)
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct GpuPtr {
    inner: u32,
}
impl GpuPtr {
    pub(crate) fn null() -> Self {
        GpuPtr { inner: u32::MAX }
    }
}

impl DescriptorHeap {
    fn create_buffer(&mut self, desc: &BufferDesc) -> GpuPtr {
        let buffer = Buffer::new(Arc::clone(&self.device), desc).unwrap();

        let free_idx = self.get_free_idx();

        self.heap_info[free_idx as usize] = HeapOwnedResource::Buffer(buffer);

        self.dirty.push(free_idx);

        GpuPtr { inner: free_idx }
    }

    fn create_image(&mut self, desc: &ImageDesc) -> GpuPtr {
        // TODO: use format
        let image = Image::new(Arc::clone(&self.device), desc);

        let free_idx = self.get_free_idx();

        self.heap_info[free_idx as usize] = HeapOwnedResource::Image(image);

        self.dirty.push(free_idx);

        GpuPtr { inner: free_idx }
    }

    fn with_mapping(&mut self, ptr: GpuPtr, f: impl Fn(&mut [u8])) {
        if let HeapOwnedResource::Buffer(ref mut buffer) = self.heap_info[ptr.inner as usize] {
            buffer.with_mapping(f);
        } else {
            panic!();
        }
    }

    fn free(&mut self, ptr: GpuPtr) {
        self.heap_info[ptr.inner as usize] = HeapOwnedResource::Empty;
        self.free_list.push(ptr.inner);
    }
}

impl BufferUsage {
    pub fn descriptor_type(&self) -> vk::DescriptorType {
        match self {
            Self::Uniform => vk::DescriptorType::UNIFORM_BUFFER,
            Self::Storage => vk::DescriptorType::STORAGE_BUFFER,
            //BufferType::Indirect => vk::DescriptorType,
            _ => panic!(),
        }
    }

    fn usage(&self) -> vk::BufferUsageFlags {
        match self {
            BufferUsage::Uniform => vk::BufferUsageFlags::UNIFORM_BUFFER,
            BufferUsage::Storage => vk::BufferUsageFlags::STORAGE_BUFFER,
            BufferUsage::Index => vk::BufferUsageFlags::INDEX_BUFFER,
            BufferUsage::DescriptorHeap => vk::BufferUsageFlags::DESCRIPTOR_HEAP_EXT,
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

pub struct Buffer {
    pub(super) device: Arc<DeviceHandles>,
    pub(super) inner: vk::Buffer,
    allocation: vk_mem::Allocation,
    size: u64,
    pub(super) ty: BufferUsage,
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
                .usage(buffer_usage.usage() | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS);

            let allocation_info = desc.memory.vma_options();

            let (buffer, allocation) = device
                .allocator
                .create_buffer(&buffer_info, &allocation_info)?;
            Ok(Self {
                device,
                inner: buffer,
                allocation,
                size: size,
                ty: buffer_usage,
            })
        }
    }

    pub fn copy_to_buffer(&self, data: &[u8], dst_offset: u64) {
        if data
            .len()
            .checked_add(dst_offset as usize)
            .expect("Buffer offset overflow")
            > self.len() as usize
        {
            panic!("")
        }
        unsafe {
            // This is safe with &self because VMA uses an internal mutex
            self.device
                .allocator
                .copy_memory_to_allocation(&self.allocation, data, dst_offset)
                .unwrap();
        }
    }

    pub fn with_mapping(&mut self, f: impl FnOnce(&mut [u8])) {
        // Safety: &mut self is required because calling any buffer function inside
        // f would create aliasing &mut
        unsafe {
            let size = self.len();
            let mapping = self
                .device
                .allocator
                .map_memory(&mut self.allocation)
                .unwrap();

            let mapping = slice::from_raw_parts_mut(mapping, size as usize);
            f(mapping);

            self.device.allocator.unmap_memory(&mut self.allocation);
        }
    }

    pub fn len(&self) -> u64 {
        self.size
    }

    unsafe fn device_address(&self) -> vk::DeviceAddress {
        let address = self
            .device
            .inner
            .get_buffer_device_address(&vk::BufferDeviceAddressInfo::default().buffer(self.inner));

        address
    }

    pub unsafe fn device_address_range(&self) -> vk::DeviceAddressRangeKHR {
        let address = self.device_address();
        let size = self.len();
        vk::DeviceAddressRangeKHR { address, size }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            println!("Destroying buffer");
            self.device
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

pub struct Image {
    pub(super) device: Arc<DeviceHandles>,
    pub(super) inner: vk::Image,
    allocation: vk_mem::Allocation,
    pub(super) view: Option<vk::ImageView>,
    pub(super) desc: ImageDesc,
    pub(super) current_layout: Cell<vk::ImageLayout>,
}

impl Image {
    // Create a 2D Image with the given extent
    pub fn new(device: Arc<DeviceHandles>, description: &ImageDesc) -> Self {
        unsafe {
            let layout = vk::ImageLayout::GENERAL;
            let image_info = vk::ImageCreateInfo::default()
                //.flags()
                .image_type(vk::ImageType::TYPE_2D)
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
                .usage(description.usage)
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

            Self {
                device,
                inner: image,
                allocation,
                view: None,
                desc: description.clone(),
                current_layout: Cell::new(layout),
            }
        }
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        unsafe {
            self.device
                .allocator
                .destroy_image(self.inner, &mut self.allocation);
        }
    }
}
