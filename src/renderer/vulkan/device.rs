use std::sync::Arc;

use ash::VkResult;
use ash::ext;
use ash::khr;
use ash::vk;
use ash::vk::TaggedStructure as _;

use crate::renderer::shader::SlangModule;
use crate::renderer::shader::reflect::ShaderInfo;
use crate::renderer::vulkan::instance::DescriptorHeapProps;

use super::*;

// We will cache stuff we check in instance creation and then use it as needed
pub struct DeviceProps {
    pub descriptor_heap: Option<DescriptorHeapProps>,
}

pub struct Device {
    pub(super) instance: Arc<Instance>,
    pub(super) physical_device: vk::PhysicalDevice,
    pub(super) inner: ash::Device,
    pub(super) queue: vk::Queue,
    pub(super) queue_family_index: u32,
    pub(crate) allocator: vk_mem::Allocator,
    pub(super) debug_utils_loader: ext::debug_utils::Device,
    pub(super) props: DeviceProps,
}

impl Device {
    pub fn create_fence(self: Arc<Device>) -> VkResult<Fence> {
        let fence = unsafe {
            self.inner
                .create_fence(&vk::FenceCreateInfo::default(), None)?
        };

        Ok(Fence {
            device: self,
            inner: fence,
        })
    }

    pub fn create_semaphore(self: Arc<Device>) -> VkResult<Semaphore> {
        let semaphore = unsafe {
            self.inner
                .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?
        };

        Ok(Semaphore {
            device: self,
            inner: semaphore,
        })
    }

    pub fn create_shader_module(self: Arc<Device>, shader: &SlangModule) -> ShaderModule {
        let code = bytemuck::try_cast_slice(shader.spirv.text.as_chunks::<4>().0).unwrap();
        let info: Option<Vec<ShaderInfo>> = shader
            .reflection
            .as_ref()
            .map(|reflection| reflection.into())
            .unwrap();

        dbg!(&info);

        let create_info = vk::ShaderModuleCreateInfo::default().code(code);
        unsafe {
            let shader_module = self.inner.create_shader_module(&create_info, None).unwrap();

            ShaderModule {
                device: self,
                shader_module,
                info: info.unwrap(),
            }
        }
    }

    pub unsafe fn create_image_view(
        self: Arc<Self>,
        image: vk::Image,
        format: vk::Format,
    ) -> VkResult<ImageView> {
        let image_view_create_info = vk::ImageViewCreateInfo::default()
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image(image);
        let image_view = unsafe { self.inner.create_image_view(&image_view_create_info, None) };

        image_view.map(|view| ImageView {
            device: self,
            inner: view,
        })
    }

    pub fn create_pipeline(
        self: Arc<Device>,
        target_format: &TargetFormat,
        shader_module: &ShaderModule,
    ) -> VkResult<Pipeline> {
        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];

        let dynamic_state =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::default();

        let pipeline_layout_create_info = vk::PipelineLayoutCreateInfo::default();

        let mut rendering_create_info = vk::PipelineRenderingCreateInfo::default()
            .view_mask(0)
            .color_attachment_formats(target_format.color)
            .depth_attachment_format(vk::Format::UNDEFINED)
            .stencil_attachment_format(vk::Format::UNDEFINED);

        let rasterization_state = vk::PipelineRasterizationStateCreateInfo::default()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::BACK) // should have this configurable
            .front_face(vk::FrontFace::CLOCKWISE)
            .depth_bias_enable(false)
            .line_width(1.0);

        let multisample_state = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1) // probably should be configurable or use dynamic state
            .sample_shading_enable(false)
            .alpha_to_coverage_enable(false) // gotta learn multisample coverage
            .alpha_to_one_enable(false);

        let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(true)
            .depth_write_enable(true)
            .depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL)
            .stencil_test_enable(false)
            .min_depth_bounds(0.0)
            .max_depth_bounds(1.0);

        let attachments = [vk::PipelineColorBlendAttachmentState::default()
            .blend_enable(false)
            .color_write_mask(vk::ColorComponentFlags::RGBA)];

        let color_blend_state = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .logic_op(vk::LogicOp::COPY)
            .attachments(&attachments);

        let pipeline_layout = unsafe {
            self.inner
                .create_pipeline_layout(&pipeline_layout_create_info, None)?
        };

        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .scissor_count(1)
            .viewport_count(1);

        let pipeline_stages: Vec<_> = shader_module
            .info
            .iter()
            .map(|info| {
                vk::PipelineShaderStageCreateInfo::default()
                    .module(shader_module.shader_module)
                    .stage(info.stage.into())
                    .name(&info.entry_point)
            })
            .collect();

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
            .layout(pipeline_layout)
            .render_pass(vk::RenderPass::null()) // hooray for dynamic rendering
            .push(&mut rendering_create_info);

        unsafe {
            let pipelines = self
                .inner
                .create_graphics_pipelines(
                    vk::PipelineCache::null(), // todo: pipeline cache
                    &[graphics_pipeline_create_info],
                    None,
                )
                .map_err(|(_, res)| res)?;

            self.inner.destroy_pipeline_layout(pipeline_layout, None);
            Ok(Pipeline {
                device: self,
                inner: pipelines[0],
            })
        }
    }

    pub fn create_command_pool(self: Arc<Self>) -> VkResult<CommandPool> {
        unsafe {
            let command_pool_create_info = &vk::CommandPoolCreateInfo::default()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(self.queue_family_index);

            let command_pool = self
                .inner
                .create_command_pool(command_pool_create_info, None)?;

            Ok(CommandPool {
                device: self,
                inner: command_pool,
                command_buffers: vec![],
            })
        }
    }

    pub fn queue_wait_idle(&self) {
        unsafe {
            self.inner.queue_wait_idle(self.queue).unwrap();
        }
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        println!("Destroying Device");
        unsafe {
            self.inner.device_wait_idle().unwrap();
            self.inner.destroy_device(None);
        }
    }
}
