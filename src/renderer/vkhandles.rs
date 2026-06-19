use std::ffi::c_char;
use std::mem::ManuallyDrop;
use std::sync::Arc;

use ash::VkResult;
use ash::ext;
use ash::khr;
use ash::vk;
use ash::vk::TaggedStructure;
use winit::raw_window_handle::RawDisplayHandle;
use winit::raw_window_handle::RawWindowHandle;

use crate::renderer::commands::*;
use crate::renderer::debug::create_debug_messenger;
use crate::renderer::debug::*;
use crate::renderer::shader::SlangModule;
use crate::renderer::shader::reflect::ShaderInfo;

pub struct Surface {
    instance: Arc<Instance>,
    inner: vk::SurfaceKHR,
    //surface_format: vk::SurfaceFormatKHR,
    //surface_resolution: vk::Extent2D,
    surface_loader: khr::surface::Instance,
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            self.instance
                .surface_loader
                .destroy_surface(self.inner, None);
        }
    }
}

pub struct Swapchain {
    device: Arc<Device>,
    surface: Arc<Surface>,
    swapchain: vk::SwapchainKHR,

    swapchain_images: Vec<vk::Image>,
    swapchain_image_views: Vec<ImageView>,
    swapchain_loader: khr::swapchain::Device,

    pub(super) swapchain_format: vk::SurfaceFormatKHR,
    swapchain_extent: vk::Extent2D,
    // Might want to do a cleaner version of this
    synchronization: Vec<PresentationSynchronization>,
}

impl Swapchain {
    pub unsafe fn new(device: Arc<Device>, surface: Arc<Surface>) -> Result<Swapchain, vk::Result> {
        Self::create_swapchain(device, surface, vk::SwapchainKHR::null())
    }

    unsafe fn create_swapchain(
        device: Arc<Device>,
        surface: Arc<Surface>,
        swapchain: vk::SwapchainKHR,
    ) -> Result<Swapchain, vk::Result> {
        const SWAPCHAIN_SIZE: u32 = 3;

        let swapchain_loader =
            khr::swapchain::Device::load(&device.instance.instance, &device.inner);

        let surface_loader = &surface.surface_loader;

        let surface_caps = surface_loader
            .get_physical_device_surface_capabilities(device.physical_device, surface.inner)?;

        if surface_caps.current_extent.height == 0 || surface_caps.current_extent.width == 0 {
            return Err(vk::Result::NOT_READY);
        }

        let surface_formats = surface_loader
            .get_physical_device_surface_formats(device.physical_device, surface.inner)?;

        // TODO: should probably fetch an RGB format instead of BGR
        let surface_format = surface_formats[0];

        let present_modes = surface_loader
            .get_physical_device_surface_present_modes(device.physical_device, surface.inner)?;

        let present_mode = if present_modes.contains(&vk::PresentModeKHR::MAILBOX) {
            vk::PresentModeKHR::MAILBOX
        } else {
            vk::PresentModeKHR::FIFO
        };

        let swapchain_create_info = vk::SwapchainCreateInfoKHR::default()
            .surface(surface.inner)
            .image_extent(surface_caps.current_extent)
            .image_format(surface_format.format)
            .image_color_space(surface_format.color_space)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(surface_caps.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE) // PRE_MULTIPLIED is funky
            .image_array_layers(1)
            .min_image_count(SWAPCHAIN_SIZE)
            .present_mode(present_mode)
            .clipped(true)
            .old_swapchain(swapchain);

        // TODO: should be able to handle this
        let swapchain = swapchain_loader
            .create_swapchain(&swapchain_create_info, None)
            .expect("Cannot create swapchain");

        let swapchain_images = swapchain_loader.get_swapchain_images(swapchain)?;

        let swapchain_image_views = swapchain_images
            .iter()
            .map(|image| Arc::clone(&device).create_image_view(*image, surface_format.format))
            .collect::<VkResult<Vec<ImageView>>>()?;

        let synchronization = {
            let mut vec = vec![];
            for _ in 0..SWAPCHAIN_SIZE {
                vec.push(PresentationSynchronization {
                    draw_fence: Fence::new_signalled(Arc::clone(&device))?,
                    render_finished: Semaphore::new(Arc::clone(&device))?,
                    present_complete: Semaphore::new(Arc::clone(&device))?,
                });
            }
            vec
        };
        Ok(Swapchain {
            device,
            surface: surface,
            swapchain,
            swapchain_images,
            swapchain_image_views,
            swapchain_loader,
            swapchain_extent: surface_caps.current_extent,
            swapchain_format: surface_format,

            synchronization,
        })
    }

    fn recreate(&mut self) -> VkResult<()> {
        unsafe {
            let new_swapchain = Swapchain::create_swapchain(
                Arc::clone(&self.device),
                Arc::clone(&self.surface),
                self.swapchain,
            );
            *self = new_swapchain?;
            Ok(())
        }
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        unsafe {
            self.device.inner.device_wait_idle().unwrap();

            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None);
        }
    }
}

pub struct PresentationEngine {
    swapchain: Swapchain,
    pool: CommandPool,
}

impl PresentationEngine {
    pub fn new(swapchain: Swapchain, mut command_pool: CommandPool) -> Self {
        command_pool
            .allocate_command_buffers(swapchain.swapchain_images.len() as u32)
            .unwrap();
        Self {
            swapchain,
            pool: command_pool,
        }
    }

    // TODO: Might wish to encapsulate the frame index
    pub fn next_frame(&mut self, frame_index: u64) -> VkResult<PresentationContext<'_>> {
        let swapchain = &mut self.swapchain;
        let length = swapchain.swapchain_images.len();
        let synchronization = &mut swapchain.synchronization[frame_index as usize % length];

        unsafe {
            let draw_fence = &mut synchronization.draw_fence;
            draw_fence.wait().unwrap();

            let acquire_info = vk::AcquireNextImageInfoKHR::default()
                .device_mask(1)
                .swapchain(swapchain.swapchain)
                .timeout(u64::MAX) // configurable?
                .semaphore(synchronization.present_complete.inner);

            let (idx, _) = swapchain
                .swapchain_loader
                .acquire_next_image2(&acquire_info)?;

            draw_fence.reset().unwrap();

            let image = swapchain.swapchain_images[idx as usize % length];
            let image_view = &swapchain.swapchain_image_views[idx as usize % length];
            let extent = swapchain.swapchain_extent;
            let render_target = RenderTarget {
                swapchain: swapchain.swapchain,
                swapchain_loader: &swapchain.swapchain_loader,
                extent: extent,
                image_idx: idx,
                color_image: image,
                color_image_view: image_view.inner,
                synchronization,
            };

            let command_buffer = self.pool.acquire_command_buffer(frame_index);

            Ok(PresentationContext {
                command_buffer,
                render_target,
            })
        }
    }

    pub(crate) fn recreate_swapchain(&mut self) -> VkResult<()> {
        self.swapchain.recreate()
    }
}

pub struct TargetFormat<'a> {
    pub color: &'a [vk::Format],
    pub depth: Option<vk::Format>,
    pub stencil: Option<vk::Format>,
}

struct PresentationSynchronization {
    draw_fence: Fence,
    render_finished: Semaphore,
    present_complete: Semaphore,
}

pub struct RenderTarget<'a> {
    swapchain: vk::SwapchainKHR,
    swapchain_loader: &'a khr::swapchain::Device,
    pub extent: vk::Extent2D,
    image_idx: u32,
    pub color_image: vk::Image,
    pub color_image_view: vk::ImageView,
    synchronization: &'a PresentationSynchronization,
}

pub struct ImageView {
    device: Arc<Device>,
    inner: vk::ImageView,
    // image: Arc<vk::Image>, // Maybe Weak would be better?
    // Or maybe a different representation would be better entirely
    // like OwningView which owns the image
}

impl Drop for ImageView {
    fn drop(&mut self) {
        unsafe {
            self.device.inner.destroy_image_view(self.inner, None);
        }
    }
}

pub struct Device {
    instance: Arc<Instance>,
    physical_device: vk::PhysicalDevice,
    pub(super) inner: ash::Device,
    queue: vk::Queue,
    pub(super) queue_family_index: u32,
    _allocator: vk_mem::Allocator,
    debug_utils_loader: ext::debug_utils::Device,
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
            .depth_clamp_enable(false) // ?
            .rasterizer_discard_enable(false) // ?
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::BACK) // should have this configurable
            .front_face(vk::FrontFace::CLOCKWISE)
            .depth_bias_enable(false) // ?
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

pub struct Pipeline {
    device: Arc<Device>,
    pub(super) inner: vk::Pipeline,
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.inner.destroy_pipeline(self.inner, None);
        }
    }
}

pub struct ShaderModule {
    device: Arc<Device>,
    shader_module: vk::ShaderModule,
    info: Vec<ShaderInfo>,
}

impl ShaderModule {}

impl Drop for ShaderModule {
    fn drop(&mut self) {
        unsafe {
            self.device
                .inner
                .destroy_shader_module(self.shader_module, None);
        }
    }
}

macro_rules! raii_handle {
    ($name:ident, $constructor:ident, $destructor:ident) => {
        pub struct $name {
            device: Arc<Device>,
            inner: vk::$name,
        }

        impl $name {
            unsafe fn new(device: Arc<Device>) -> VkResult<Self> {
                device.$constructor()
            }
        }

        impl Drop for $name {
            fn drop(&mut self) {
                unsafe {
                    self.device.inner.$destructor(self.inner, None);
                }
            }
        }
    };
}

raii_handle! {Fence, create_fence, destroy_fence}
raii_handle! {Semaphore, create_semaphore, destroy_semaphore}

vk_debug_name_trait_impl! {Semaphore}
vk_debug_name_trait_impl! {Fence}

impl Fence {
    pub fn new_signalled(device: Arc<Device>) -> VkResult<Self> {
        // SAFETY:
        // - device is valid and has one queue
        unsafe {
            let fence = device.inner.create_fence(
                &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                None,
            )?;
            Ok(Self {
                device,
                inner: fence,
            })
        }
    }

    pub fn wait(&mut self) -> VkResult<()> {
        // SAFETY:
        // - device is valid
        // - fence is valid and acquired from device
        unsafe {
            self.device
                .inner
                .wait_for_fences(&[self.inner], true, u64::MAX)
        }
    }

    pub fn reset(&mut self) -> VkResult<()> {
        // SAFETY:
        // - device is valid
        // - fence is valid and acquired from device
        unsafe { self.device.inner.reset_fences(&[self.inner]) }
    }
}

//struct Fence {
//    device: Arc<Device>,
//    inner: vk::Fence,
//}
//
//impl Drop for Fence {
//    fn drop(&mut self) {
//        unsafe {
//            self.device.device.destroy_fence(self.inner, None);
//        }
//    }
//}

pub struct CommandPool {
    device: Arc<Device>,
    inner: vk::CommandPool,
    command_buffers: Vec<CommandBuffer>,
}

impl CommandPool {
    pub fn allocate_command_buffers(&mut self, number_of_command_buffers: u32) -> VkResult<()> {
        let allocate_info = vk::CommandBufferAllocateInfo::default()
            .command_buffer_count(number_of_command_buffers)
            .command_pool(self.inner);

        unsafe {
            let mut command_buffers = self
                .device
                .inner
                .allocate_command_buffers(&allocate_info)?
                .into_iter()
                .map(|cb| CommandBuffer {
                    device: Arc::clone(&self.device),
                    inner: cb,
                })
                .collect::<Vec<_>>();
            self.command_buffers.append(&mut command_buffers);
        }
        Ok(())
    }

    // grab next command buffer for rendering
    pub fn acquire_command_buffer(&self, frame_index: u64) -> &CommandBuffer {
        let idx = frame_index as usize % self.command_buffers.len();

        &self.command_buffers[idx]
    }
}

impl Drop for CommandPool {
    fn drop(&mut self) {
        unsafe {
            self.device.inner.destroy_command_pool(self.inner, None);
        }
    }
}

pub struct CommandBuffer {
    device: Arc<Device>,
    // interesting question. We could store an Arc to CommandPool and have it free after all
    // command buffers or own all command buffers within CommandPool and destroy CommandBuffers when CommandPool is dropped
    inner: vk::CommandBuffer,
}
unsafe impl ActiveCommandBuffer for CommandBuffer {
    unsafe fn get_device(&self) -> &Device {
        &self.device
    }

    unsafe fn get_command_buffer(&self) -> vk::CommandBuffer {
        self.inner
    }
}
impl SynchronizationCommands for CommandBuffer {}

pub struct RecordingCommandBuffer<'a> {
    inner: &'a CommandBuffer,
}

impl RecordingCommandBuffer<'_> {
    pub fn render(&self, rendering_info: &vk::RenderingInfo, f: impl Fn(&Self)) {
        self.cmd_begin_rendering(rendering_info);
        f(self);
        self.cmd_end_rendering();
    }
}

unsafe impl ActiveCommandBuffer for RecordingCommandBuffer<'_> {
    unsafe fn get_device(&self) -> &Device {
        &self.inner.device
    }

    unsafe fn get_command_buffer(&self) -> vk::CommandBuffer {
        self.inner.inner
    }
}
impl DrawCommands for RecordingCommandBuffer<'_> {}
impl DynamicRenderingCommands for RecordingCommandBuffer<'_> {}
impl BindingCommands for RecordingCommandBuffer<'_> {}
impl SynchronizationCommands for RecordingCommandBuffer<'_> {}

impl CommandBuffer {
    pub fn reset(&self) -> VkResult<()> {
        unsafe {
            self.device
                .inner
                .reset_command_buffer(self.inner, vk::CommandBufferResetFlags::RELEASE_RESOURCES)
        }
    }

    pub fn record(&self, f: impl Fn(&RecordingCommandBuffer)) -> VkResult<()> {
        unsafe {
            self.reset()?;

            let begin_info = &vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

            self.device
                .inner
                .begin_command_buffer(self.inner, begin_info)?;

            f(&RecordingCommandBuffer { inner: self });

            self.device.inner.end_command_buffer(self.inner)?;
        }
        Ok(())
    }
}

impl Drop for CommandBuffer {
    fn drop(&mut self) {
        // hmmm
    }
}

pub struct PresentationContext<'a> {
    command_buffer: &'a CommandBuffer,
    render_target: RenderTarget<'a>,
}

impl<'a> PresentationContext<'a> {
    pub fn submit_and_present(
        self,
        f: impl Fn(&CommandBuffer, &RenderTarget) -> VkResult<()>,
    ) -> VkResult<()> {
        let _ = f(self.command_buffer, &self.render_target)?;
        let command_buffer = &self.command_buffer;
        let render_target = &self.render_target;

        let submit_info =
            [vk::CommandBufferSubmitInfo::default().command_buffer(command_buffer.inner)];
        let render_finish_semaphore = [vk::SemaphoreSubmitInfo::default()
            .semaphore(render_target.synchronization.render_finished.inner)
            .stage_mask(vk::PipelineStageFlags2::BOTTOM_OF_PIPE)];
        let present_complete_semaphore = [vk::SemaphoreSubmitInfo::default()
            .semaphore(render_target.synchronization.present_complete.inner)
            .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)];
        let submits = [vk::SubmitInfo2::default()
            .command_buffer_infos(&submit_info)
            .signal_semaphore_infos(&render_finish_semaphore)
            .wait_semaphore_infos(&present_complete_semaphore)];

        unsafe {
            command_buffer
                .device
                .inner
                .queue_submit2(
                    command_buffer.device.queue,
                    &submits,
                    render_target.synchronization.draw_fence.inner,
                )
                .unwrap();

            let swapchains = [render_target.swapchain];
            let wait_semaphores = [render_target.synchronization.render_finished.inner];
            let indices = [render_target.image_idx];
            let present_info = vk::PresentInfoKHR::default()
                .swapchains(&swapchains)
                .wait_semaphores(&wait_semaphores)
                .image_indices(&indices);

            render_target
                .swapchain_loader
                .queue_present(command_buffer.device.queue, &present_info)?;
        }
        Ok(())
    }
}

pub struct Instance {
    entry: ManuallyDrop<ash::Entry>,
    instance: ash::Instance,

    debug_utils_loader: ext::debug_utils::Instance,
    debug_messenger: vk::DebugUtilsMessengerEXT,

    surface_loader: khr::surface::Instance,
}

impl Instance {
    pub unsafe fn new(raw_display_handle: RawDisplayHandle) -> Self {
        let entry = ash::Entry::load().expect("Failed to load Vulkan");
        let app_name = c"Ark Renderer";

        let layer_names = [c"VK_LAYER_KHRONOS_validation"];
        let layers_names_raw: Vec<*const c_char> = layer_names
            .iter()
            .map(|raw_name| raw_name.as_ptr())
            .collect();

        let mut extension_names = ash_window::enumerate_required_extensions(raw_display_handle)
            .unwrap()
            .to_vec();
        extension_names.push(ext::debug_utils::NAME.as_ptr());

        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            extension_names.push(ash::khr::portability_enumeration::NAME.as_ptr());
            // Enabling this extension is a requirement when using `VK_KHR_portability_subset`
            extension_names.push(ash::khr::get_physical_device_properties2::NAME.as_ptr());
        }

        let appinfo = vk::ApplicationInfo::default()
            .application_name(app_name)
            .application_version(0)
            .engine_name(app_name)
            .engine_version(0)
            .api_version(vk::make_api_version(0, 1, 4, 0));

        let create_flags = if cfg!(any(target_os = "macos", target_os = "ios")) {
            vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR
        } else {
            vk::InstanceCreateFlags::default()
        };

        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&appinfo)
            .enabled_layer_names(&layers_names_raw)
            .enabled_extension_names(&extension_names)
            .flags(create_flags);

        let instance: ash::Instance = entry
            .create_instance(&create_info, None)
            .expect("Instance creation error");

        let debug_utils_loader = ext::debug_utils::Instance::load(&entry, &instance);
        let debug_messenger = create_debug_messenger(&debug_utils_loader);

        let surface_loader = ash::khr::surface::Instance::load(&entry, &instance);

        Self {
            entry: ManuallyDrop::new(entry),
            instance,

            debug_utils_loader,
            debug_messenger,

            surface_loader,
        }
    }

    pub unsafe fn create_surface(
        self: Arc<Self>,
        raw_display_handle: RawDisplayHandle,
        raw_window_handle: RawWindowHandle,
    ) -> Surface {
        let surface =
            ash_window::SurfaceFactory::new(&self.entry, &self.instance, raw_display_handle)
                .unwrap()
                .create_surface(raw_window_handle, None)
                .unwrap();

        let surface_loader = khr::surface::Instance::load(&self.entry, &self.instance);

        Surface {
            instance: self,
            inner: surface,
            surface_loader,
        }
    }

    pub unsafe fn create_device(self: Arc<Self>, surface: &Surface) -> Device {
        let pdevices = self
            .instance
            .enumerate_physical_devices()
            .expect("Physical device error");

        for pdevice in &pdevices {
            let mut device_properties = Default::default();
            self.instance
                .get_physical_device_properties2(*pdevice, &mut device_properties);

            let mut device_features = vk::PhysicalDeviceFeatures2::default();
            self.instance
                .get_physical_device_features2(*pdevice, &mut device_features);

            println!(
                "Device {pdevice:?}\nProperties:\n{device_properties:#?}\nFeatures:\n{device_features:#?}"
            );
        }

        let (physical_device, queue_family_index) = pdevices
            .iter()
            .find_map(|pdevice| {
                self.instance
                    .get_physical_device_queue_family_properties(*pdevice)
                    .iter()
                    .enumerate()
                    .find_map(|(index, info)| {
                        let supports_graphic_and_surface =
                            info.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                                && self
                                    .surface_loader
                                    .get_physical_device_surface_support(
                                        *pdevice,
                                        index as u32,
                                        surface.inner,
                                    )
                                    .unwrap();
                        if supports_graphic_and_surface {
                            Some((*pdevice, index))
                        } else {
                            None
                        }
                    })
            })
            .expect("Couldn't find suitable device.");
        let queue_family_index = queue_family_index as u32;

        // TODO: add extensions we want
        let enabled_extension_names = [
            // I like the theoretical possibility of running headless, but I am never going
            // to run this on an actually headless platform
            khr::swapchain::NAME.as_ptr(),
            khr::synchronization2::NAME.as_ptr(),
            // ext::device_fault::NAME.as_ptr(), // device errors
            // ext::shader_object::NAME.as_ptr(), // replace pipelines with shader objects
            // ext::descriptor_heap::NAME.as_ptr(), // replace descriptor indexing with heaps
            // ext::mesh_shader::NAME.as_ptr(), // mesh shaders, weeee
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            ash::khr::portability_subset::NAME.as_ptr(),
        ];

        let vk10_features = vk::PhysicalDeviceFeatures::default().pipeline_statistics_query(true);
        let mut vk11_features =
            vk::PhysicalDeviceVulkan11Features::default().shader_draw_parameters(true);
        let mut vk12_features = vk::PhysicalDeviceVulkan12Features::default();
        let mut vk13_features = vk::PhysicalDeviceVulkan13Features::default()
            .dynamic_rendering(true)
            .synchronization2(true);
        let mut enabled_features = vk::PhysicalDeviceFeatures2::default()
            .features(vk10_features)
            .push(&mut vk11_features)
            .push(&mut vk12_features)
            .push(&mut vk13_features);

        let queue_create_infos = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family_index)
            .queue_priorities(&[0.5])];

        let device_create_info = vk::DeviceCreateInfo::default()
            .enabled_extension_names(&enabled_extension_names)
            .queue_create_infos(&queue_create_infos)
            .extend(&mut enabled_features);

        let device = self
            .instance
            .create_device(physical_device, &device_create_info, None)
            .expect("Failed to create device");

        let allocator_create_info =
            vk_mem::AllocatorCreateInfo::new(&self.instance, &device, physical_device);
        let allocator =
            vk_mem::Allocator::new(allocator_create_info).expect("Failed to create allocator");

        let queue_info = vk::DeviceQueueInfo2::default()
            .queue_family_index(queue_family_index)
            .queue_index(0);

        let queue = device.get_device_queue2(&queue_info);
        // let swapchain_loader = khr::swapchain::Device::new(&self.instance, &device);
        let debug_utils_loader = ext::debug_utils::Device::load(&self.instance, &device);

        Device {
            instance: self,
            physical_device,
            inner: device,
            queue,
            queue_family_index,
            _allocator: allocator,
            debug_utils_loader,
        }
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        println!("Destroying Instance");
        unsafe {
            self.debug_utils_loader
                .destroy_debug_utils_messenger(self.debug_messenger, None);

            self.instance.destroy_instance(None);

            ManuallyDrop::drop(&mut self.entry);
        }
    }
}
