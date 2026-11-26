use std::ffi::CStr;
use std::mem::ManuallyDrop;
use std::sync::Arc;
use std::sync::RwLock;

use ash::ext;
//use ash::khr;
use ash::vk;
use ash::vk::TaggedStructure as _;

use super::command::PipelineType;
use super::descriptor_indexing::DescriptorIndexingData;
use super::gpu_resources::GpuResources;
use super::instance::DescriptorHeapProps;
use super::instance::DeviceResult;

use super::instance::Instance;
use super::queue::Queue;
use super::swapchain::Swapchain;

use crate::instance::DeviceCreateInfo;
use crate::*;

pub struct ShaderIR<'a> {
    pub bytes: &'a [u32],
    pub entry: &'a CStr,
}

pub(super) struct DeviceHandles {
    pub inner: ash::Device,
    pub instance: Instance,
    pub pdevice: vk::PhysicalDevice,
    pub allocator: ManuallyDrop<vk_mem::Allocator>,
    pub limits: vk::PhysicalDeviceLimits,
    // Extensions
    pub debug_utils: ext::debug_utils::Device,
    pub descriptor_heap: Option<ext::descriptor_heap::Device>,
    pub descriptor_heap_props: Option<DescriptorHeapProps>,
    pub extended_dynamic_state3: Option<ext::extended_dynamic_state3::Device>,
    // pub device_address_commands: khr::device_address_commands::Device, // Poor support right now
    pub descriptor_indexing: Option<DescriptorIndexingData>,
    pub mesh_shader: Option<ext::mesh_shader::Device>,
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
            // Pipelines have an Arc to DeviceHandles so this is fine
            if let Some(descriptor_indexing) = &mut self.descriptor_indexing {
                descriptor_indexing.destroy(&self.inner);
            }
            // Must drop the allocator before the device
            ManuallyDrop::drop(&mut self.allocator);
            self.inner.destroy_device(None);
        }
    }
}

pub struct Device {
    handles: Arc<DeviceHandles>,
    heap: Arc<RwLock<GpuResources>>,
}

impl Device {
    pub fn new(create_info: &DeviceCreateInfo) -> Self {
        unsafe {
            let instance = Instance::new_with_presentation(
                create_info.presentation.unwrap_or(WindowSystem::None),
            );

            let DeviceResult {
                device,
                pdevice,
                _graphics_queue_index: _,
                _compute_queue_index: _,
                _transfer_queue_index: _,
            } = instance.create_device(create_info);

            let features = create_info.device_features;

            let mut allocator_create_info =
                vk_mem::AllocatorCreateInfo::new(&instance.instance, &device, pdevice);
            allocator_create_info.flags = vk_mem::AllocatorCreateFlags::BUFFER_DEVICE_ADDRESS;
            let allocator =
                vk_mem::Allocator::new(allocator_create_info).expect("Failed to create allocator");

            let debug_utils_loader = ext::debug_utils::Device::load(&instance.instance, &device);

            let descriptor_heap_props = instance.get_descriptor_heap_properties(&pdevice);

            let descriptor_heap_loader = if features.descriptor_heap {
                Some(ext::descriptor_heap::Device::load(
                    &instance.instance,
                    &device,
                ))
            } else {
                None
            };

            let properties = instance.instance.get_physical_device_properties(pdevice);

            let extended_dynamic_state3 = if features.dynamic_blend_state {
                Some(ext::extended_dynamic_state3::Device::load(
                    &instance.instance,
                    &device,
                ))
            } else {
                None
            };

            let descriptor_indexing = Some(DescriptorIndexingData::new(&device));

            let mesh_shader = if false {
                Some(ext::mesh_shader::Device::load(&instance.instance, &device))
            } else {
                None
            };

            let handles = Arc::new(DeviceHandles {
                instance,
                inner: device,
                pdevice,
                limits: properties.limits,
                allocator: ManuallyDrop::new(allocator),
                debug_utils: debug_utils_loader,
                descriptor_heap: descriptor_heap_loader,
                descriptor_heap_props: descriptor_heap_props,
                extended_dynamic_state3,
                descriptor_indexing,
                mesh_shader,
            });

            let descriptor_heap = GpuResources::new(Arc::clone(&handles)).unwrap();

            Self {
                handles: handles,
                heap: Arc::new(RwLock::new(descriptor_heap)),
            }
        }
    }

    fn device(&self) -> &ash::Device {
        &self.handles.inner
    }

    /// Pipeline layout for new pipelines. With descriptor heaps no layout is needed,
    /// otherwise the descriptor indexing layout is used.
    fn get_pipeline_layout(&self) -> vk::PipelineLayout {
        if self.handles.descriptor_heap.is_some() {
            vk::PipelineLayout::null()
        } else if let Some(di) = &self.handles.descriptor_indexing {
            di.pipeline_layout
        } else {
            vk::PipelineLayout::null()
        }
    }

    pub fn get_descriptor_heap_properties(&self) -> DescriptorHeapProps {
        self.handles
            .descriptor_heap_props
            .as_ref()
            .expect("Descriptor heap feature is not enabled!")
            .clone()
    }

    pub fn set_object_name(&self, obj: GpuPtr, name: &CStr) {
        let heap = self.heap.read().unwrap();

        if let Some(buffer) = heap.buffers.get(&obj.handle) {
            self.handles.set_object_name(buffer.inner, name)
        }
    }

    pub fn set_image_name(&self, image: GpuImage, name: &CStr) {
        let heap = self.heap.read().unwrap();

        if let Some(image) = heap.images.get(&image.handle) {
            self.handles.set_object_name(image.inner, name);
            if let Some(view) = image.view {
                self.handles.set_object_name(view, name);
            }
        }
    }
}

impl DeviceRHI for Device {
    type Pipeline = Pipeline;
    type Semaphore = Semaphore;
    type Queue = Queue;
    type GpuPtr = GpuPtr;

    fn create_buffer(&mut self, details: &BufferInfo) -> Self::GpuPtr {
        self.heap.write().unwrap().create_buffer(details)
    }

    fn create_image(&mut self, details: &ImageInfo) -> GpuImage {
        self.heap.write().unwrap().create_image(details)
    }

    fn buffer_host_addr(&self, ptr: Self::GpuPtr) -> *mut u8 {
        let heap = self.heap.read().unwrap();

        let buffer = heap.ptr_to_buffer(ptr);

        // TODO: check alignment and size
        unsafe { buffer.mapped_ptr.unwrap().byte_add(ptr.offset as usize) }
    }

    fn buffer_device_addr(&self, ptr: Self::GpuPtr) -> u64 {
        let heap = self.heap.read().unwrap();

        let buffer = heap.ptr_to_buffer(ptr);

        let info = vk::BufferDeviceAddressInfo::default().buffer(buffer.inner);
        unsafe {
            let addr = self.handles.inner.get_buffer_device_address(&info);
            // TODO: check alignment and size
            addr + (ptr.offset as u64)
        }
    }

    unsafe fn destroy_buffer(&mut self, ptr: Self::GpuPtr) {
        if ptr.is_null() {
            return;
        }

        self.heap.write().unwrap().free_buffer(ptr);
    }

    unsafe fn destroy_image(&mut self, image: GpuImage) {
        if image.is_null() {
            return;
        }

        self.heap.write().unwrap().free_image(image);
    }

    fn create_queue(
        &mut self,
        ty: QueueType,
        command_pools: u32,
        command_buffers_per_pool: u32,
    ) -> Self::Queue {
        match ty {
            QueueType::Graphics => Queue::new(
                &self.handles,
                &self.heap,
                0,
                command_pools,
                command_buffers_per_pool,
            ),
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

    unsafe fn create_compute_pipeline(&mut self, compute_ir: &ShaderIR) -> Self::Pipeline {
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
            .layout(self.get_pipeline_layout())];

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

    unsafe fn create_graphics_pipeline(
        &mut self,
        vertex_ir: &ShaderIR,
        fragment_ir: &ShaderIR,
        description: &RasterDescription,
    ) -> Self::Pipeline {
        let dynamic_states = [
            vk::DynamicState::VIEWPORT,
            vk::DynamicState::SCISSOR,
            // depth state is always dynamic
            vk::DynamicState::DEPTH_TEST_ENABLE,
            vk::DynamicState::DEPTH_WRITE_ENABLE,
            vk::DynamicState::DEPTH_COMPARE_OP,
            vk::DynamicState::DEPTH_BOUNDS_TEST_ENABLE,
            vk::DynamicState::STENCIL_TEST_ENABLE,
            vk::DynamicState::STENCIL_OP,
            vk::DynamicState::DEPTH_BOUNDS,
            vk::DynamicState::DEPTH_BIAS_ENABLE,
            vk::DynamicState::DEPTH_BIAS,
            // blend state depends on input arguments
            vk::DynamicState::COLOR_BLEND_ENABLE_EXT,
            vk::DynamicState::COLOR_BLEND_EQUATION_EXT,
            vk::DynamicState::COLOR_WRITE_MASK_EXT,
        ];
        let dynamic_blending = self.handles.extended_dynamic_state3.is_some();

        let dynamic_states = if dynamic_blending {
            dynamic_states.as_slice()
        } else {
            &dynamic_states[0..11]
        };

        let dynamic_state =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(description.topology.into());

        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_attribute_descriptions(&[])
            .vertex_binding_descriptions(&[]);

        let color_formats: Vec<_> = description
            .color_formats
            .iter()
            .map(|&format| format.into())
            .collect();

        let mut rendering_create_info = vk::PipelineRenderingCreateInfo::default()
            .color_attachment_formats(&color_formats)
            .depth_attachment_format(description.depth_format.into())
            .stencil_attachment_format(description.stencil_format.into());

        let (cull_mode, front_face) = description.cull.to_vk();
        let rasterization_state = vk::PipelineRasterizationStateCreateInfo::default()
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(cull_mode)
            .front_face(front_face)
            .line_width(1.0);

        let multisample_state = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(description.samples.into())
            .sample_shading_enable(false)
            .alpha_to_coverage_enable(description.alpha_to_coverage)
            .alpha_to_one_enable(false);

        let color_blend_attachments = if let Some(blend) = description.blend_state {
            [vk::PipelineColorBlendAttachmentState::default()
                .color_blend_op(blend.color_op.into())
                .src_color_blend_factor(blend.src_color_factor.into())
                .dst_color_blend_factor(blend.dst_color_factor.into())
                .alpha_blend_op(blend.alpha_op.into())
                .src_alpha_blend_factor(blend.src_alpha_factor.into())
                .dst_alpha_blend_factor(blend.dst_alpha_factor.into())
                .color_write_mask(vk::ColorComponentFlags::RGBA)]
        } else {
            [vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::RGBA)]
        };

        let color_blend_state = if !dynamic_blending {
            vk::PipelineColorBlendStateCreateInfo::default().attachments(&color_blend_attachments)
        } else {
            vk::PipelineColorBlendStateCreateInfo::default()
        };

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

        let mut graphics_pipeline_flags = vk::PipelineCreateFlags2CreateInfo::default().flags(
            if self.handles.descriptor_heap.is_some() {
                vk::PipelineCreateFlags2::DESCRIPTOR_HEAP_EXT
            } else {
                vk::PipelineCreateFlags2::empty()
            },
        );

        let pipeline_layout = self.get_pipeline_layout();

        let graphics_pipeline_create_info = vk::GraphicsPipelineCreateInfo::default()
            .flags(vk::PipelineCreateFlags::empty())
            .stages(&pipeline_stages)
            .vertex_input_state(&vertex_input_state)
            .input_assembly_state(&input_assembly_state)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterization_state)
            .multisample_state(&multisample_state)
            .color_blend_state(&color_blend_state)
            .dynamic_state(&dynamic_state)
            .layout(pipeline_layout)
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
                ty: PipelineType::Graphics {
                    samples: description.samples,
                },
            }
        }
    }

    unsafe fn create_meshlet_pipeline(
        &mut self,
        task_ir: Option<&ShaderIR>,
        meshlet_ir: &ShaderIR,
        fragment_ir: &ShaderIR,
        description: &RasterDescription,
    ) -> Self::Pipeline {
        let dynamic_states = [
            vk::DynamicState::VIEWPORT,
            vk::DynamicState::SCISSOR,
            // depth state is always dynamic
            vk::DynamicState::DEPTH_TEST_ENABLE,
            vk::DynamicState::DEPTH_WRITE_ENABLE,
            vk::DynamicState::DEPTH_COMPARE_OP,
            vk::DynamicState::DEPTH_BOUNDS_TEST_ENABLE,
            vk::DynamicState::STENCIL_TEST_ENABLE,
            vk::DynamicState::STENCIL_OP,
            vk::DynamicState::DEPTH_BOUNDS,
            vk::DynamicState::DEPTH_BIAS_ENABLE,
            vk::DynamicState::DEPTH_BIAS,
            // blend state depends on input arguments
            vk::DynamicState::COLOR_BLEND_ENABLE_EXT,
            vk::DynamicState::COLOR_BLEND_EQUATION_EXT,
            vk::DynamicState::COLOR_WRITE_MASK_EXT,
        ];

        let dynamic_blending = self.handles.extended_dynamic_state3.is_some();

        let dynamic_states = if dynamic_blending {
            dynamic_states.as_slice()
        } else {
            &dynamic_states[0..11]
        };

        let dynamic_state =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(description.topology.into());

        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_attribute_descriptions(&[])
            .vertex_binding_descriptions(&[]);

        let color_formats: Vec<_> = description
            .color_formats
            .iter()
            .map(|&format| format.into())
            .collect();

        let mut rendering_create_info = vk::PipelineRenderingCreateInfo::default()
            .color_attachment_formats(&color_formats)
            .depth_attachment_format(description.depth_format.into())
            .stencil_attachment_format(description.stencil_format.into());

        let (cull_mode, front_face) = description.cull.to_vk();
        let rasterization_state = vk::PipelineRasterizationStateCreateInfo::default()
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(cull_mode)
            .front_face(front_face)
            .line_width(1.0);

        let multisample_state = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(description.samples.into())
            .sample_shading_enable(false)
            .alpha_to_coverage_enable(description.alpha_to_coverage)
            .alpha_to_one_enable(false);

        let color_blend_attachments = if let Some(blend) = description.blend_state {
            [vk::PipelineColorBlendAttachmentState::default()
                .color_blend_op(blend.color_op.into())
                .src_color_blend_factor(blend.src_color_factor.into())
                .dst_color_blend_factor(blend.dst_color_factor.into())
                .alpha_blend_op(blend.alpha_op.into())
                .src_alpha_blend_factor(blend.src_alpha_factor.into())
                .dst_alpha_blend_factor(blend.dst_alpha_factor.into())]
        } else {
            [vk::PipelineColorBlendAttachmentState::default()]
        };

        let color_blend_state = if !dynamic_blending {
            vk::PipelineColorBlendStateCreateInfo::default().attachments(&color_blend_attachments)
        } else {
            vk::PipelineColorBlendStateCreateInfo::default()
        };

        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .scissor_count(1)
            .viewport_count(1);

        let mut task_shader = vk::ShaderModuleCreateInfo::default()
            .code(task_ir.map_or(&[], |task_ir| bytemuck::cast_slice(task_ir.bytes)));
        let mut meshlet_shader =
            vk::ShaderModuleCreateInfo::default().code(bytemuck::cast_slice(meshlet_ir.bytes));
        let mut fragment_shader =
            vk::ShaderModuleCreateInfo::default().code(bytemuck::cast_slice(fragment_ir.bytes));

        let pipeline_stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::TASK_EXT)
                .name(&task_ir.map_or(c"", |task_ir| task_ir.entry))
                .push(&mut task_shader),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::MESH_EXT)
                .name(&meshlet_ir.entry)
                .push(&mut meshlet_shader),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .name(&fragment_ir.entry)
                .push(&mut fragment_shader),
        ];

        let mut graphics_pipeline_flags = vk::PipelineCreateFlags2CreateInfo::default().flags(
            if self.handles.descriptor_heap.is_some() {
                vk::PipelineCreateFlags2::DESCRIPTOR_HEAP_EXT
            } else {
                vk::PipelineCreateFlags2::empty()
            },
        );

        let pipeline_layout = self.get_pipeline_layout();

        let graphics_pipeline_create_info = vk::GraphicsPipelineCreateInfo::default()
            .flags(vk::PipelineCreateFlags::empty())
            .stages(if task_ir.is_some() {
                &pipeline_stages
            } else {
                &pipeline_stages[1..]
            })
            .vertex_input_state(&vertex_input_state)
            .input_assembly_state(&input_assembly_state)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterization_state)
            .multisample_state(&multisample_state)
            .color_blend_state(&color_blend_state)
            .dynamic_state(&dynamic_state)
            .layout(pipeline_layout)
            .render_pass(vk::RenderPass::null()) // VK_KHR_dynamic_rendering
            .push(&mut rendering_create_info)
            .push(&mut graphics_pipeline_flags);
        unsafe {
            let pipelines = self
                .device()
                .create_graphics_pipelines(
                    vk::PipelineCache::null(),
                    &[graphics_pipeline_create_info],
                    None,
                )
                .map_err(|(_, res)| res)
                .unwrap();
            Pipeline {
                device: Arc::clone(&self.handles),
                inner: pipelines[0],
                ty: PipelineType::Graphics {
                    samples: description.samples,
                },
            }
        }
    }

    fn get_image_descriptor(&self, image: GpuImage) -> ImageDescriptor {
        let heap = self.heap.read().unwrap();
        let mut descriptor = ImageDescriptor { inner: [0u64; 4] };
        heap.write_image_descriptor(image, &mut descriptor);
        descriptor
    }

    fn get_sampler_descriptor(&self, desc: &SamplerInfo) -> SamplerDescriptor {
        let heap = self.heap.read().unwrap();
        let mut descriptor = SamplerDescriptor { inner: [0u64; 4] };
        heap.write_sampler_descriptor(desc, &mut descriptor);
        descriptor
    }

    fn write_sampled_image_descriptor(
        &mut self,
        index: usize,
        image: GpuImage,
    ) {
        let device = self.device();
        let heap = self.heap.read().unwrap();
        if let Some(ref descriptors) = self.handles.descriptor_indexing {
            unsafe {
                let descriptor_set = descriptors.get_sampled_image_set();
                let descriptor_type = vk::DescriptorType::SAMPLED_IMAGE;

                let image = heap.get_image(image);
                let image_info = [vk::DescriptorImageInfo::default()
                    .image_layout(image.current_layout.get())
                    .image_view(image.view.unwrap())];

                let descriptor_writes = [vk::WriteDescriptorSet::default()
                    .descriptor_count(1)
                    .dst_set(descriptor_set)
                    .dst_binding(0)
                    .dst_array_element(index as u32)
                    .descriptor_type(descriptor_type)
                    .image_info(&image_info)];

                device.update_descriptor_sets(&descriptor_writes, &[]);
            }
        }
    }

    fn write_sampler_descriptor(&mut self, index: usize, sampler: &SamplerInfo) {
        let device = self.device();
        let heap = self.heap.read().unwrap();
        if let Some(ref descriptors) = self.handles.descriptor_indexing {
            unsafe {
                let sampler = heap.get_or_create_sampler(sampler);
                let image_info = [vk::DescriptorImageInfo::default().sampler(sampler)];

                let descriptor_writes = [vk::WriteDescriptorSet::default()
                    .descriptor_count(1)
                    .dst_set(descriptors.get_sampler_set())
                    .dst_binding(0)
                    .dst_array_element(index as u32)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .image_info(&image_info)];

                device.update_descriptor_sets(&descriptor_writes, &[]);
            }
        }
    }

    fn write_storage_image_descriptor(&mut self, index: usize, image: GpuImage) {
        let device = self.device();
        let heap = self.heap.read().unwrap();
        if let Some(ref descriptors) = self.handles.descriptor_indexing {
            unsafe {
                let descriptor_set = descriptors.get_storage_image_set();
                let descriptor_type = vk::DescriptorType::STORAGE_IMAGE;

                let image = heap.get_image(image);
                let image_info = [vk::DescriptorImageInfo::default()
                    // Storage images always use GENERAL layout
                    .image_layout(vk::ImageLayout::GENERAL)
                    .image_view(image.view.unwrap())];

                let descriptor_writes = [vk::WriteDescriptorSet::default()
                    .descriptor_count(1)
                    .dst_set(descriptor_set)
                    .dst_binding(0)
                    .dst_array_element(index as u32)
                    .descriptor_type(descriptor_type)
                    .image_info(&image_info)];

                device.update_descriptor_sets(&descriptor_writes, &[]);
            }
        }
    }

    fn invalidate_memory_region(&self, ptr: Self::GpuPtr, location: MemoryLocation) {
        let heap = self.heap.read().unwrap();
        let buffer = heap.ptr_to_buffer(ptr);
        let allocator = &self.handles.allocator;
        match location {
            MemoryLocation::Host => allocator
                .invalidate_allocation(&buffer.allocation, ptr.offset as u64, ptr.size as u64)
                .unwrap(),
            MemoryLocation::Device => allocator
                .flush_allocation(&buffer.allocation, ptr.offset as u64, ptr.size as u64)
                .unwrap(),
        };
    }
}

impl SwapchainDeviceRHI for Device {
    type Swapchain = Swapchain;
    type Queue = <Self as DeviceRHI>::Queue;

    fn create_swapchain(
        &self,
        queue: &crate::Queue,
        window: &crate::WindowSystemData,
        info: &crate::SwapchainInfo,
    ) -> Result<Self::Swapchain, Error> {
        unsafe {
            let handles = &self.handles;
            let heap = Arc::clone(&self.heap);
            let present_queue = queue.queue();
            let swapchain =
                Swapchain::new(Arc::clone(&handles), heap, window, present_queue, info)?;
            Ok(swapchain)
        }
    }

    fn enumerate_surface_formats(&self, surface: &WindowSystemData) -> Vec<SurfaceFormat> {
        unsafe {
            let surface = self
                .handles
                .instance
                .create_surface(surface.display_handle, surface.window_handle);

            let surface_info = vk::PhysicalDeviceSurfaceInfo2KHR::default().surface(surface.inner);

            let loader = &self.handles.instance.surface_capabilities2_loader;

            let len = loader
                .get_physical_device_surface_formats2_len(self.handles.pdevice, &surface_info)
                .unwrap();

            let mut formats = vec![vk::SurfaceFormat2KHR::default(); len];

            loader
                .get_physical_device_surface_formats2(
                    self.handles.pdevice,
                    &surface_info,
                    &mut formats,
                )
                .unwrap();

            formats
                .iter()
                .filter_map(|format| {
                    let color_space = format.surface_format.color_space.try_into().ok()?;
                    let format = Format::from(format.surface_format.format);
                    if matches!(format, Format::Undefined) {
                        return None;
                    }
                    Some(SurfaceFormat {
                        format,
                        color_space,
                    })
                })
                .collect()
        }
    }
}
