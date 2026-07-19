use core::slice;
use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::CStr;
use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::ptr::null_mut;
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
use raw_window_handle::RawDisplayHandle;
use raw_window_handle::RawWindowHandle;
use vk_mem::Alloc;

use super::command::PipelineType;
use super::command::SemaphoreInfo;
use super::instance::DescriptorHeapProps;
use super::instance::DeviceResult;
use super::swapchain::NextFrame;

use super::instance::Instance;
use super::swapchain::{Surface, Swapchain};

use super::*;

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
pub struct Semaphore {
    pub(super) inner: vk::Semaphore,
}

pub struct ShaderIR<'a> {
    pub bytes: &'a [u32],
    pub entry: &'a CStr,
}

pub(super) struct DeviceHandles {
    pub surface: Surface,
    pub inner: ash::Device,
    pub instance: Instance,
    pub pdevice: vk::PhysicalDevice,
    pub allocator: ManuallyDrop<vk_mem::Allocator>,
    pub debug_utils: ext::debug_utils::Device,
    pub descriptor_heap: ext::descriptor_heap::Device,
    pub descriptor_heap_props: DescriptorHeapProps,
    pub extended_dynamic_state3: ext::extended_dynamic_state3::Device,
    pub device_address_commands: khr::device_address_commands::Device,
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

    fn destroy(&self, device: &ash::Device) {
        unsafe {
            device.destroy_command_pool(self.command_pool, None);
        }
    }
}

struct QueuePool {
    family_index: u32,
    queue: vk::Queue,
    queues_used: usize,
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
                family_index: queue_index,
                queue,
                queues_used: 0,
                command_pools: command_pools,
            }
        }
    }
}

pub struct Device {
    handles: Arc<DeviceHandles>,
    // Inner reference should be Weak maybe?
    heap: Arc<RwLock<DescriptorHeap>>,
    swapchain: Option<Arc<RwLock<Swapchain>>>,
}

impl Device {
    pub fn new() -> Self {
        todo!()
    }

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

            let device_address_commands =
                khr::device_address_commands::Device::load(&instance.instance, &device);

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
                device_address_commands,
            });

            let descriptor_heap = DescriptorHeap::new(Arc::clone(&handles)).unwrap();

            let swapchain = Swapchain::new(Arc::clone(&handles)).unwrap();
            Self {
                handles: handles,
                heap: Arc::new(RwLock::new(descriptor_heap)),
                swapchain: Some(Arc::new(RwLock::new(swapchain))),
            }
        }
    }

    fn device(&self) -> &ash::Device {
        &self.handles.inner
    }

    pub fn get_descriptor_heap_properties(&self) -> DescriptorHeapProps {
        self.handles.descriptor_heap_props.clone()
    }
}

impl DeviceRHI for Device {
    type Pipeline = super::Pipeline;
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
                swapchain: self.swapchain.as_ref().map(|s| Arc::downgrade(&s)),
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
        meshlet_ir: &ShaderIR,
        fragment_ir: &ShaderIR,
        description: &RasterDescription,
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

pub struct Queue {
    device: Weak<DeviceHandles>,
    heap: Weak<RwLock<DescriptorHeap>>,
    queue: QueuePool,
    swapchain: Option<Weak<RwLock<Swapchain>>>,
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

    pub fn begin_recording_presentation(
        &mut self,
        command_pool: u32,
        frame_index: u64,
    ) -> Result<<Self as QueueRHI>::CommandBuffer, Error> {
        let swapchain = self.swapchain.as_ref().unwrap().upgrade().unwrap();
        let mut swapchain = swapchain.write().unwrap();
        let next_frame = match swapchain.next_frame(frame_index) {
            Ok(next_frame) => next_frame,
            Err(err) => {
                swapchain.recreate()?;
                return Err(Error::SwapchainOutOfDate);
            }
        };

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
        command_buffer.present = Some(next_frame);
        Ok(command_buffer)
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

fn find_frame(cbs: &[CommandBuffer]) -> Option<(&CommandBuffer, &NextFrame)> {
    cbs.iter()
        .find_map(|cb| cb.present.as_ref().and_then(|present| Some((cb, present))))
}

impl Drop for Queue {
    fn drop(&mut self) {
        let device = self.device.upgrade().unwrap();

        unsafe {
            for command_pool in &self.queue.command_pools {
                command_pool.destroy(&device.inner)
            }
        }
    }
}

impl QueueRHI for Queue {
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

    fn submit(&mut self, command_buffers: &[Self::CommandBuffer]) -> Result<(), Error> {
        let frame = find_frame(command_buffers);

        if let Some((cb, frame)) = frame {
            unsafe {
                cb.transition_image_layout(
                    frame.image.image,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    vk::ImageLayout::PRESENT_SRC_KHR,
                    vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
                    vk::AccessFlags2::empty(),
                );
            }
        }

        self.submit_impl(command_buffers)?;

        if let Some((_, frame)) = frame {
            let swapchain = self.swapchain.as_ref().unwrap().upgrade().unwrap();
            let mut swapchain = swapchain.write().unwrap();

            let queue = &self.queue;

            let swapchains = [swapchain.swapchain];
            let wait_semaphores = [frame.submit_signal_present_wait];
            let indices = [frame.image_idx];
            let present_info = vk::PresentInfoKHR::default()
                .swapchains(&swapchains)
                .wait_semaphores(&wait_semaphores)
                .image_indices(&indices);
            unsafe {
                let result = swapchain
                    .swapchain_loader
                    .queue_present(queue.queue, &present_info);
                if result.is_err() {
                    swapchain.recreate(); // if not ready, we will just try again next time
                }
            }
        }
        Ok(())
    }
}

enum HeapOwnedResource {
    Buffer(Buffer),
    Image(Image),
    Empty,
}

pub(super) struct DescriptorHeap {
    allocations: HashMap<vk_mem::RawAllocationHandle, HeapOwnedResource>,
    device: Arc<DeviceHandles>,
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

        Ok(Self {
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

            let samplers = [
                vk::SamplerCreateInfo::default()
                    //.flags(vk::SamplerCreateFlags::DESCRIPTOR_BUFFER_CAPTURE_REPLAY_EXT)
                    .mag_filter(vk::Filter::LINEAR) // Expose
                    .min_filter(vk::Filter::LINEAR) // Expose
                    .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                    .address_mode_u(vk::SamplerAddressMode::REPEAT)
                    .address_mode_v(vk::SamplerAddressMode::REPEAT)
                    .address_mode_w(vk::SamplerAddressMode::REPEAT)
                    .anisotropy_enable(false)
                    .max_anisotropy(0.0)
                    .mip_lod_bias(1.0)
                    .min_lod(0.0)
                    .max_lod(0.0)
                    .compare_enable(false)
                    .compare_op(vk::CompareOp::EQUAL)
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
        let props = &device.descriptor_heap_props;
        unsafe {
            let resource = [vk::ResourceDescriptorInfoEXT::default()
                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                .data(vk::ResourceDescriptorDataEXT {
                    p_image: &vk::ImageDescriptorInfoEXT {
                        p_view: &vk::ImageViewCreateInfo::default()
                            .view_type(image_type_to_image_view_type(image.desc.ty))
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
        match self
            .allocations
            .get(&(ptr.addr as vk_mem::RawAllocationHandle))
        {
            Some(HeapOwnedResource::Buffer(buffer)) => buffer,
            _ => panic!(),
        }
    }

    pub fn ptr_to_image(&self, ptr: GpuPtr) -> &Image {
        match self
            .allocations
            .get(&(ptr.addr as vk_mem::RawAllocationHandle))
        {
            Some(HeapOwnedResource::Image(image)) => image,
            _ => panic!(),
        }
    }
}

impl Drop for DescriptorHeap {
    fn drop(&mut self) {
        unsafe {
            self.device.inner.device_wait_idle().unwrap();
        }
    }
}

impl GpuPtr {
    pub fn null() -> Self {
        Self {
            addr: null_mut(),
            offset: 0,
            size: 0,
        }
    }

    pub fn is_null(&self) -> bool {
        self.addr.is_null()
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct GpuPtr {
    addr: *mut u8,
    pub offset: u32,
    pub size: u32,
}

impl DescriptorHeap {
    fn create_buffer(&mut self, desc: &BufferDesc) -> GpuPtr {
        let buffer = Buffer::new(Arc::clone(&self.device), desc).unwrap();

        let raw = buffer.allocation.get_raw();
        let size = buffer.size as u32;
        self.allocations
            .insert(raw, HeapOwnedResource::Buffer(buffer));

        GpuPtr {
            addr: raw as *mut u8,
            offset: 0,
            size,
        }
    }

    fn create_image(&mut self, desc: &ImageDesc) -> GpuPtr {
        // TODO: use format
        let image = Image::new(Arc::clone(&self.device), desc);

        let raw = image.allocation.get_raw();

        let size = image.size as u32;
        self.allocations
            .insert(raw, HeapOwnedResource::Image(image));

        GpuPtr {
            addr: raw as *mut u8,
            offset: 0,
            size,
        }
    }

    fn free(&mut self, ptr: GpuPtr) {
        let res = self
            .allocations
            .remove(&(ptr.addr as vk_mem::RawAllocationHandle));

        if let None = res {
            panic!("Double free.");
        }
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

pub(super) struct Buffer {
    pub device: Arc<DeviceHandles>,
    pub inner: vk::Buffer,
    allocation: vk_mem::Allocation,
    size: u64,
    pub ty: BufferUsage,
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
                        | vk::BufferUsageFlags::TRANSFER_SRC,
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
                device,
                inner: buffer,
                allocation,
                size: size,
                ty: buffer_usage,
                mapped_ptr,
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
            if let Some(_) = self.mapped_ptr {
                self.device.allocator.unmap_memory(&mut self.allocation);
            }

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

fn image_type_to_image_view_type(ty: vk::ImageType) -> vk::ImageViewType {
    match ty {
        vk::ImageType::TYPE_1D => vk::ImageViewType::TYPE_1D,
        vk::ImageType::TYPE_2D => vk::ImageViewType::TYPE_2D,
        vk::ImageType::TYPE_3D => vk::ImageViewType::TYPE_3D,
        _ => unreachable!(),
    }
}

pub struct Image {
    pub(super) device: Arc<DeviceHandles>,
    pub(super) inner: vk::Image,
    allocation: vk_mem::Allocation,
    pub(super) view: Option<vk::ImageView>,
    pub(super) desc: ImageDesc,
    pub(super) current_layout: Cell<vk::ImageLayout>,
    pub(super) size: usize,
}

impl Image {
    // Create a 2D Image with the given extent
    pub fn new(device: Arc<DeviceHandles>, description: &ImageDesc) -> Self {
        unsafe {
            let layout = vk::ImageLayout::UNDEFINED;
            let image_info = vk::ImageCreateInfo::default()
                //.flags()
                .image_type(description.ty)
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
                .usage(description.usage | vk::ImageUsageFlags::TRANSFER_DST)
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

            Self {
                device,
                inner: image,
                allocation,
                view: None,
                desc: description.clone(),
                current_layout: Cell::new(layout),
                size: memory_req.size as usize,
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
