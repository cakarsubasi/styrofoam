use std::sync::Arc;

use ash::VkResult;
use ash::ext;
use ash::khr;
use ash::vk;

use super::*;

pub struct Surface {
    pub(super) instance: Arc<Instance>,
    pub(super) inner: vk::SurfaceKHR,
    //surface_format: vk::SurfaceFormatKHR,
    //surface_resolution: vk::Extent2D,
    pub(super) surface_loader: khr::surface::Instance,
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

    pub swapchain_format: vk::SurfaceFormatKHR,
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

pub struct TargetFormat<'a> {
    pub color: &'a [vk::Format],
    pub depth: Option<vk::Format>,
    pub stencil: Option<vk::Format>,
}
