use std::sync::Arc;

use ash::VkResult;
use ash::ext;
use ash::khr;
use ash::vk;

use crate::renderer::vulkan::device::DeviceHandles;

pub struct Surface {
    pub(super) inner: vk::SurfaceKHR,
    //surface_format: vk::SurfaceFormatKHR,
    //surface_resolution: vk::Extent2D,
    pub(super) surface_loader: khr::surface::Instance,
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            self.surface_loader.destroy_surface(self.inner, None);
        }
    }
}

pub struct Swapchain {
    device: Arc<DeviceHandles>,
    pub(super) swapchain: vk::SwapchainKHR,

    pub(super) swapchain_loader: khr::swapchain::Device,

    pub swapchain_format: vk::SurfaceFormatKHR,
    swapchain_extent: vk::Extent2D,

    resources: PresentationResources,
}

#[derive(Clone, Copy)]
pub struct SwapchainImage {
    pub(super) image: vk::Image,
    pub(super) view: vk::ImageView,
    pub(super) extent: vk::Extent2D,
    pub(super) format: vk::Format,
}

pub struct PresentationResources {
    images: Vec<SwapchainImage>,            // swapchain_size
    acquire_semaphores: Vec<vk::Semaphore>, // frames_in_flight
    submit_semaphores: Vec<vk::Semaphore>,  // swapchain_size
}

impl PresentationResources {
    fn maximum_frames_in_flight(&self) -> usize {
        self.acquire_semaphores.len()
    }

    fn swapchain_size(&self) -> usize {
        self.images.len()
    }
}

impl Swapchain {
    pub unsafe fn new(device: Arc<DeviceHandles>) -> Result<Swapchain, vk::Result> {
        Self::create_swapchain(device, vk::SwapchainKHR::null())
    }

    unsafe fn create_swapchain(
        device: Arc<DeviceHandles>,
        swapchain: vk::SwapchainKHR,
    ) -> Result<Swapchain, vk::Result> {
        const MAXIMUM_FRAMES_IN_FLIGHT: u32 = 2;
        const SWAPCHAIN_SIZE: u32 = MAXIMUM_FRAMES_IN_FLIGHT + 1;

        let swapchain_loader =
            khr::swapchain::Device::load(&device.instance.instance, &device.inner);

        let surface_loader = &device.surface.surface_loader;

        let surface_caps = surface_loader
            .get_physical_device_surface_capabilities(device.pdevice, device.surface.inner)?;

        if surface_caps.current_extent.height == 0 || surface_caps.current_extent.width == 0 {
            return Err(vk::Result::NOT_READY);
        }

        let surface_format = Self::choose_surface_format(&device, surface_loader, &device.surface)?;

        let present_modes = surface_loader
            .get_physical_device_surface_present_modes(device.pdevice, device.surface.inner)?;

        let present_mode = if present_modes.contains(&vk::PresentModeKHR::MAILBOX) {
            vk::PresentModeKHR::MAILBOX
        } else {
            vk::PresentModeKHR::FIFO
        };

        let swapchain_create_info = vk::SwapchainCreateInfoKHR::default()
            .surface(device.surface.inner)
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

        let swapchain_images = swapchain_images
            .into_iter()
            .map(|image| {
                let view = device.inner.create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(surface_format.format)
                        .image(image)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        }),
                    None,
                )?;
                Ok(SwapchainImage {
                    image,
                    view,
                    format: surface_format.format,
                    extent: surface_caps.current_extent,
                })
            })
            .collect::<VkResult<Vec<SwapchainImage>>>()?;

        let acquire_semaphores = (0..MAXIMUM_FRAMES_IN_FLIGHT)
            .map(|_| {
                device
                    .inner
                    .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
            })
            .collect::<VkResult<Vec<_>>>()?;
        let submit_semaphores = (0..SWAPCHAIN_SIZE)
            .map(|_| {
                device
                    .inner
                    .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
            })
            .collect::<VkResult<Vec<_>>>()?;

        let resources = PresentationResources {
            images: swapchain_images,
            acquire_semaphores,
            submit_semaphores,
        };

        Ok(Swapchain {
            device,
            swapchain,
            swapchain_loader,
            swapchain_extent: surface_caps.current_extent,
            swapchain_format: surface_format,
            resources,
        })
    }

    fn choose_surface_format(
        device: &DeviceHandles,
        surface_loader: &khr::surface::Instance,
        surface: &Surface,
    ) -> VkResult<vk::SurfaceFormatKHR> {
        unsafe {
            let surface_formats = surface_loader
                .get_physical_device_surface_formats(device.pdevice, surface.inner)?;

            let surface_format = surface_formats
                .iter()
                .find(|&format| {
                    format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
                        && format.format == vk::Format::R8G8B8A8_SRGB
                })
                .unwrap_or(&surface_formats[0]);

            Ok(*surface_format)
        }
    }

    pub(crate) fn recreate(&mut self) -> VkResult<()> {
        unsafe {
            let new_swapchain =
                Swapchain::create_swapchain(Arc::clone(&self.device), self.swapchain);
            *self = new_swapchain?;
            Ok(())
        }
    }

    // TODO: Might wish to encapsulate the frame index
    pub fn next_frame(&self, frame_index: u64) -> VkResult<NextFrame> {
        let frame_idx = frame_index as usize % self.resources.maximum_frames_in_flight();

        let acquire_semaphore = self.resources.acquire_semaphores[frame_idx];

        unsafe {
            let acquire_info = vk::AcquireNextImageInfoKHR::default()
                .device_mask(1)
                .swapchain(self.swapchain)
                .timeout(u64::MAX)
                .semaphore(acquire_semaphore);

            let (image_idx, _) = self.swapchain_loader.acquire_next_image2(&acquire_info)?;

            let submit_semaphore = self.resources.submit_semaphores[image_idx as usize];
            let swapchain_image = self.resources.images[image_idx as usize];

            let extent = self.swapchain_extent;

            let next_frame = NextFrame {
                image: swapchain_image,
                image_idx,
                submit_wait: acquire_semaphore,
                submit_signal_present_wait: submit_semaphore,
            };

            Ok(next_frame)
        }
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        unsafe {
            self.device.inner.device_wait_idle().unwrap();

            for semaphore in &self.resources.acquire_semaphores {
                self.device.inner.destroy_semaphore(*semaphore, None);
            }

            for semaphore in &self.resources.submit_semaphores {
                self.device.inner.destroy_semaphore(*semaphore, None);
            }

            for image in &self.resources.images {
                self.device.inner.destroy_image_view(image.view, None);
            }

            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None);
        }
    }
}

pub struct NextFrame {
    pub(super) image: SwapchainImage,
    pub(super) image_idx: u32,
    pub(super) submit_wait: vk::Semaphore,
    pub(super) submit_signal_present_wait: vk::Semaphore,
}
