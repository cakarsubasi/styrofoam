use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::CStr;
use std::hash::Hash;
use std::mem::ManuallyDrop;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use ash::VkResult;
use ash::ext;
//use ash::khr;
use ash::vk;
use ash::vk::TaggedStructure as _;
use vk_mem::Alloc;

use super::command::PipelineType;
use super::instance::DescriptorHeapProps;
use super::instance::DeviceResult;

use super::instance::Instance;
use super::queue::Queue;
use super::swapchain::Swapchain;

use crate::instance::DeviceCreateInfo;
use crate::*;

pub(super) struct DescriptorIndexingData {
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: [vk::DescriptorSet; 2],
    descriptor_set_layouts: [vk::DescriptorSetLayout; 2],
    pub pipeline_layout: vk::PipelineLayout,
}

impl DescriptorIndexingData {
    fn new(device: &ash::Device) -> Self {
        unsafe {
            let binding_size = 4096;

            let all_stages = vk::ShaderStageFlags::VERTEX
                | vk::ShaderStageFlags::FRAGMENT
                | vk::ShaderStageFlags::COMPUTE;

            let descriptor_pool = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(2)
                        .flags(vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND)
                        .pool_sizes(&[
                            vk::DescriptorPoolSize::default()
                                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                                .descriptor_count(binding_size),
                            vk::DescriptorPoolSize::default()
                                .ty(vk::DescriptorType::STORAGE_IMAGE)
                                .descriptor_count(binding_size),
                        ]),
                    None,
                )
                .unwrap();

            let push_constant_ranges = [vk::PushConstantRange::default()
                .offset(0)
                .size(256)
                .stage_flags(all_stages)];

            let binding_flags = vk::DescriptorBindingFlags::PARTIALLY_BOUND
                | vk::DescriptorBindingFlags::UPDATE_AFTER_BIND
                | vk::DescriptorBindingFlags::UPDATE_UNUSED_WHILE_PENDING
                | vk::DescriptorBindingFlags::VARIABLE_DESCRIPTOR_COUNT;
            let binding_flags = [binding_flags];

            let sampled_image_bindings = [vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_count(binding_size)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .stage_flags(all_stages)];
            let storage_image_bindings = [vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_count(binding_size)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .stage_flags(all_stages)];

            let mut binding_flags_create_info =
                vk::DescriptorSetLayoutBindingFlagsCreateInfo::default()
                    .binding_flags(&binding_flags);

            let sampled_image_descriptor_set_layout = device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default()
                        .bindings(&sampled_image_bindings)
                        .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
                        .extend(&mut binding_flags_create_info),
                    None,
                )
                .unwrap();
            let storage_image_descriptor_set_layout = device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default()
                        .bindings(&storage_image_bindings)
                        .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
                        .extend(&mut binding_flags_create_info),
                    None,
                )
                .unwrap();

            let layouts = [
                sampled_image_descriptor_set_layout,
                storage_image_descriptor_set_layout,
            ];

            let binding_sizes = [binding_size, binding_size];
            let mut allocate_info = vk::DescriptorSetVariableDescriptorCountAllocateInfo::default()
                .descriptor_counts(&binding_sizes);
            let descriptor_sets = device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(descriptor_pool)
                        .set_layouts(&layouts)
                        .extend(&mut allocate_info),
                )
                .unwrap();
            let descriptor_sets = *descriptor_sets.as_array::<2>().unwrap();

            let pipeline_layout = device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .push_constant_ranges(&push_constant_ranges)
                        .set_layouts(&layouts),
                    None,
                )
                .unwrap();

            Self {
                descriptor_pool,
                descriptor_sets,
                descriptor_set_layouts: layouts,
                pipeline_layout,
            }
        }
    }

    fn get_sampled_image_set(&self) -> vk::DescriptorSet {
        self.descriptor_sets[0]
    }

    fn get_storage_image_set(&self) -> vk::DescriptorSet {
        self.descriptor_sets[1]
    }

    fn destroy(&mut self, device: &ash::Device) {
        unsafe {
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            // Destroying the pool implicitly frees the descriptor sets allocated from it
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            for layout in self.descriptor_set_layouts {
                device.destroy_descriptor_set_layout(layout, None);
            }
        }
    }
}

pub struct ShaderIR<'a> {
    pub bytes: &'a [u32],
    pub entry: &'a CStr,
}

pub(super) struct PushDescriptorData {
    graphics_descriptor_set_layout: vk::DescriptorSetLayout,
    pub graphics_pipeline_layout: vk::PipelineLayout,
    compute_descriptor_set_layout: vk::DescriptorSetLayout,
    pub compute_pipeline_layout: vk::PipelineLayout,
}

impl PushDescriptorData {
    const BINDING_COUNT: usize = 32;

    fn new_empty(device: &ash::Device) -> Self {
        Self::from_size::<0>(device)
    }

    fn new(device: &ash::Device) -> Self {
        Self::from_size::<{ Self::BINDING_COUNT }>(device)
    }

    fn from_size<const BINDING_SIZE: usize>(device: &ash::Device) -> Self {
        let graphics_push_constant_range = vk::PushConstantRange::default()
            .offset(0)
            .size(256)
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT);

        let compute_push_constant_range = vk::PushConstantRange::default()
            .offset(0)
            .size(256)
            .stage_flags(vk::ShaderStageFlags::COMPUTE);

        let mut graphics_bindings = [vk::DescriptorSetLayoutBinding::default()
            .descriptor_count(1)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT);
            BINDING_SIZE];

        let mut compute_bindings = [vk::DescriptorSetLayoutBinding::default()
            .descriptor_count(1)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .stage_flags(vk::ShaderStageFlags::COMPUTE);
            BINDING_SIZE];

        for i in 0..BINDING_SIZE {
            graphics_bindings[i].binding = i as u32;
            compute_bindings[i].binding = i as u32;
        }

        unsafe {
            let graphics_descriptor_set_layout = device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default()
                        .bindings(&graphics_bindings)
                        .flags(vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR),
                    None,
                )
                .unwrap();

            let graphics_pipeline_layout = device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .push_constant_ranges(&[graphics_push_constant_range])
                        .set_layouts(&[graphics_descriptor_set_layout]),
                    None,
                )
                .unwrap();

            let compute_descriptor_set_layout = device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default()
                        .bindings(&compute_bindings)
                        .flags(vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR),
                    None,
                )
                .unwrap();

            let compute_pipeline_layout = device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .push_constant_ranges(&[compute_push_constant_range])
                        .set_layouts(&[compute_descriptor_set_layout]),
                    None,
                )
                .unwrap();

            Self {
                graphics_pipeline_layout,
                graphics_descriptor_set_layout,
                compute_pipeline_layout,
                compute_descriptor_set_layout,
            }
        }
    }

    fn destroy(&mut self, device: &ash::Device) {
        unsafe {
            device.destroy_pipeline_layout(self.graphics_pipeline_layout, None);
            device.destroy_pipeline_layout(self.compute_pipeline_layout, None);
            device.destroy_descriptor_set_layout(self.graphics_descriptor_set_layout, None);
            device.destroy_descriptor_set_layout(self.compute_descriptor_set_layout, None);
        }
    }
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
    pub push_descriptor: Option<PushDescriptorData>,
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
            if let Some(pd) = &mut self.push_descriptor {
                pd.destroy(&self.inner);
            }
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
    heap: Arc<RwLock<DescriptorHeap>>,
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

            let push_descriptor = if features.push_descriptor {
                Some(PushDescriptorData::new(&device))
            } else if !features.descriptor_heap {
                Some(PushDescriptorData::new_empty(&device))
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
                push_descriptor,
                descriptor_indexing,
                mesh_shader,
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
        self.handles
            .descriptor_heap_props
            .as_ref()
            .expect("Descriptor heap feature is not enabled!")
            .clone()
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

    fn create_buffer(&mut self, details: &BufferInfo) -> Self::GpuPtr {
        self.heap.write().unwrap().create_buffer(details)
    }

    fn create_image(&mut self, details: &ImageInfo) -> Self::GpuPtr {
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

    unsafe fn destroy_ptr(&mut self, ptr: Self::GpuPtr) {
        self.heap.write().unwrap().free(ptr);
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
            vk::DynamicState::DEPTH_WRITE_ENABLE,
            vk::DynamicState::DEPTH_TEST_ENABLE,
            vk::DynamicState::DEPTH_BIAS_ENABLE,
            vk::DynamicState::DEPTH_BIAS,
            vk::DynamicState::DEPTH_COMPARE_OP,
            // blend state depends on input arguments
            vk::DynamicState::COLOR_BLEND_ENABLE_EXT,
            vk::DynamicState::COLOR_BLEND_EQUATION_EXT,
            vk::DynamicState::COLOR_WRITE_MASK_EXT,
        ];
        let dynamic_blending = self.handles.extended_dynamic_state3.is_some();

        let dynamic_states = if dynamic_blending {
            dynamic_states.as_slice()
        } else {
            &dynamic_states[0..7]
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
            .rasterization_samples(vk::SampleCountFlags::from_raw(description.samples as u32))
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

        let pipeline_layout = if self.handles.descriptor_heap.is_some() {
            vk::PipelineLayout::null()
        } else if let Some(pd) = &self.handles.push_descriptor {
            pd.graphics_pipeline_layout
        } else {
            vk::PipelineLayout::null()
        };

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
                ty: PipelineType::Graphics,
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
            vk::DynamicState::DEPTH_WRITE_ENABLE,
            vk::DynamicState::DEPTH_TEST_ENABLE,
            vk::DynamicState::DEPTH_BIAS_ENABLE,
            vk::DynamicState::DEPTH_BIAS,
            vk::DynamicState::DEPTH_COMPARE_OP,
            // blend state depends on input arguments
            vk::DynamicState::COLOR_BLEND_ENABLE_EXT,
            vk::DynamicState::COLOR_BLEND_EQUATION_EXT,
            vk::DynamicState::COLOR_WRITE_MASK_EXT,
        ];

        let dynamic_blending = self.handles.extended_dynamic_state3.is_some();

        let dynamic_states = if dynamic_blending {
            dynamic_states.as_slice()
        } else {
            &dynamic_states[0..7]
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
            .rasterization_samples(vk::SampleCountFlags::from_raw(description.samples as u32))
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

        let pipeline_layout = if self.handles.descriptor_heap.is_some() {
            vk::PipelineLayout::null()
        } else if let Some(pd) = &self.handles.push_descriptor {
            pd.graphics_pipeline_layout
        } else {
            vk::PipelineLayout::null()
        };

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
                ty: PipelineType::Graphics,
            }
        }
    }

    fn get_image_descriptor(&self, image: Self::GpuPtr) -> ImageDescriptor {
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
        image: GpuPtr,
        sampler: &SamplerInfo,
    ) {
        let device = self.device();
        let heap = self.heap.read().unwrap();
        if let Some(ref descriptors) = self.handles.descriptor_indexing {
            unsafe {
                let descriptor_set = descriptors.get_sampled_image_set();
                let descriptor_type = vk::DescriptorType::COMBINED_IMAGE_SAMPLER;

                let sampler = heap.get_or_create_sampler(sampler);
                let image = heap.ptr_to_image(image);
                let image_info = [vk::DescriptorImageInfo::default()
                    .image_layout(image.current_layout.get())
                    .image_view(image.view.unwrap())
                    .sampler(sampler)];

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

    fn write_storage_image_descriptor(&mut self, index: usize, image: GpuPtr) {
        let device = self.device();
        let heap = self.heap.read().unwrap();
        if let Some(ref descriptors) = self.handles.descriptor_indexing {
            unsafe {
                let descriptor_set = descriptors.get_storage_image_set();
                let descriptor_type = vk::DescriptorType::STORAGE_IMAGE;

                let image = heap.ptr_to_image(image);
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

// Might consider having two hash maps for either type and even splitting GpuPtr
pub(super) enum HeapOwnedResource {
    Buffer(Buffer),
    Image(Image),
}

fn quantize(num: f32, min: f32, max: f32, bits: u32) -> u64 {
    assert!(max > min);
    assert!(bits > 0 && bits < 63);

    let num = num.clamp(min, max);

    let max_int = (1u64 << bits) - 1;
    let t = (num - min) / (max - min);

    (t as f64 * max_int as f64).round() as u64
}

impl Hash for SamplerInfo {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.mag_filter.hash(state);
        self.min_filter.hash(state);
        self.mipmap_mode.hash(state);
        self.address_mode.hash(state);
        let anisotropy = quantize(self.anisotropy, 1.0, 16.0, 32);
        anisotropy.hash(state);
        // Are these good values? Who knows!
        let lod_bias = quantize(self.lod_bias, -1e6, 16.0, 32);
        lod_bias.hash(state);
        let lod_range_lo = quantize(self.lod_range[0], -1e6, 1e6, 32);
        let lod_range_hi = quantize(self.lod_range[1], -1e6, 1e6, 32);
        lod_range_lo.hash(state);
        lod_range_hi.hash(state);
        self.compare_op.hash(state);
    }
}

impl Eq for SamplerInfo {}

impl SamplerInfo {
    fn to_vk(&self) -> vk::SamplerCreateInfo<'_> {
        let anisotropy_enable = self.anisotropy > 1.0;
        let max_anisotropy = if anisotropy_enable {
            self.anisotropy
        } else {
            0.0
        };
        let compare_enable = self.compare_op.is_some();
        let compare_op = if let Some(compare_op) = self.compare_op {
            compare_op
        } else {
            CompareOp::Never // doesn't matter
        };
        vk::SamplerCreateInfo::default()
            //.flags(vk::SamplerCreateFlags::DESCRIPTOR_BUFFER_CAPTURE_REPLAY_EXT)
            .mag_filter(self.mag_filter.into())
            .min_filter(self.min_filter.into())
            .mipmap_mode(self.mipmap_mode.into())
            .address_mode_u(self.address_mode[0].into())
            .address_mode_v(self.address_mode[1].into())
            .address_mode_w(self.address_mode[2].into())
            .anisotropy_enable(anisotropy_enable)
            .max_anisotropy(max_anisotropy)
            .mip_lod_bias(self.lod_bias)
            .min_lod(self.lod_range[0])
            .max_lod(self.lod_range[1])
            .compare_enable(compare_enable)
            .compare_op(compare_op.into())
            .border_color(vk::BorderColor::FLOAT_OPAQUE_WHITE)
            .unnormalized_coordinates(false)
    }
}

pub(super) struct DescriptorHeap {
    handle_counter: AtomicU64,
    allocations: HashMap<u64, HeapOwnedResource>,
    device: Arc<DeviceHandles>,
    sampler_cache: RwLock<HashMap<SamplerInfo, vk::Sampler>>,
}

unsafe impl Send for DescriptorHeap {}
unsafe impl Sync for DescriptorHeap {}

impl DescriptorHeap {
    pub fn new(device: Arc<DeviceHandles>) -> VkResult<Self> {
        Ok(Self {
            handle_counter: AtomicU64::new(1),
            device,
            allocations: HashMap::new(),
            sampler_cache: RwLock::new(HashMap::new()),
        })
    }

    /// For use with VK_KHR_push_descriptor. Take &self and use separate locking just in case. Cache samplers
    /// since they are immutable and can be reused. They should also be relatively cheap to have and we can
    /// drop them all at once. Probably don't care enough about this path to use LRU or pruning strategies.
    pub(super) fn get_or_create_sampler(&self, info: &SamplerInfo) -> vk::Sampler {
        let lock = self.sampler_cache.read().unwrap();

        if let Some(sampler) = lock.get(info) {
            *sampler
        } else {
            drop(lock);
            let mut lock = self.sampler_cache.write().unwrap();

            unsafe {
                let sampler = self
                    .device
                    .inner
                    .create_sampler(&info.to_vk(), None)
                    .unwrap();

                lock.insert(*info, sampler);

                sampler
            }
        }
    }

    fn write_sampler_descriptor(&self, desc: &SamplerInfo, addr: &mut SamplerDescriptor) {
        let device = &self.device;
        let descriptor_heap = &device.descriptor_heap;

        unsafe {
            let descriptor = [vk::HostAddressRangeEXT::default()
                .address(bytemuck::cast_slice_mut(&mut addr.inner))];

            let samplers = [desc.to_vk()];

            descriptor_heap
                .as_ref()
                .expect("Descriptor heap feature is not enabled.")
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
                            .format(image.desc.format.into())
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
                .as_ref()
                .expect("Descriptor heap feature is not enabled.")
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

    fn create_buffer(&mut self, desc: &BufferInfo) -> GpuPtr {
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

    fn create_image(&mut self, desc: &ImageInfo) -> GpuPtr {
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
            Memory::HostVisible => vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::Auto,
                flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
                ..Default::default()
            },
            Memory::DeviceLocal => vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::AutoPreferDevice,
                ..Default::default()
            },
            Memory::HostCoherent => vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::Auto,
                flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_RANDOM,
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
    pub fn new(device: Arc<DeviceHandles>, desc: &BufferInfo) -> VkResult<Self> {
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

            let mapped_ptr = if let Memory::DeviceLocal = desc.memory {
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

impl ImageInfo {
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

impl ImageUsage {
    fn to_vk(&self) -> vk::ImageUsageFlags {
        return vk::ImageUsageFlags::INPUT_ATTACHMENT
            | vk::ImageUsageFlags::COLOR_ATTACHMENT
            | vk::ImageUsageFlags::STORAGE
            | vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::TRANSFER_DST;
    }
}

pub(super) struct AllocatedImageData {
    pub size: usize,
    pub allocation: vk_mem::Allocation,
}

pub(super) struct SwapchainImageData {
    pub idx: u32,
    pub submit_wait: Cell<vk::Semaphore>,
    pub submit_signal_present_wait: Cell<vk::Semaphore>,
}

pub(super) enum ImageData {
    Allocated(AllocatedImageData),
    Swapchain(SwapchainImageData),
}

pub(super) struct Image {
    pub inner: vk::Image,
    pub view: Option<vk::ImageView>,
    pub desc: ImageInfo,
    pub current_layout: Cell<vk::ImageLayout>,
    pub data: ImageData,
}

impl Image {
    // Create a 2D Image with the given extent
    pub fn new(device: Arc<DeviceHandles>, description: &ImageInfo) -> Self {
        unsafe {
            let layout = vk::ImageLayout::UNDEFINED;
            let image_info = vk::ImageCreateInfo::default()
                .image_type(description.ty.into())
                .format(description.format.into())
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
                .initial_layout(layout);

            let allocation_info = vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::Auto,
                ..Default::default()
            };

            let (image, allocation) = device
                .allocator
                .create_image(&image_info, &allocation_info)
                .unwrap();

            let memory_req = device.inner.get_image_memory_requirements(image);

            let view = match description.usage {
                ImageUsage::Attachment => Some(
                    device
                        .inner
                        .create_image_view(
                            &vk::ImageViewCreateInfo::default()
                                .view_type(description.ty.into())
                                .format(description.format.into())
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
            ImageData::Swapchain(_swapchain_image_data) => 9999999999, // need to figure this out
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
