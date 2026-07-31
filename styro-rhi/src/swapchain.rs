use std::cell::Cell;
use std::ffi::CString;
use std::sync::Arc;
use std::sync::RwLock;

use ash::VkResult;
use ash::khr;
use ash::vk;

use crate::Error;
use crate::GpuPtr;
use crate::ImageDesc;
use crate::SwapchainInfo;
use crate::SwapchainRHI;
use crate::WindowSystemData;
use crate::device::DescriptorHeap;
use crate::device::Image;
use crate::device::ImageData;

use super::device::DeviceHandles;

pub(super) struct Surface {
    pub inner: vk::SurfaceKHR,
    //surface_format: vk::SurfaceFormatKHR,
    //surface_resolution: vk::Extent2D,
    pub surface_loader: khr::surface::Instance,
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            self.surface_loader.destroy_surface(self.inner, None);
        }
    }
}

pub struct SwapchainHandle {
    device: Arc<DeviceHandles>,
    swapchain: vk::SwapchainKHR,
    swapchain_loader: khr::swapchain::Device,
}

impl Drop for SwapchainHandle {
    fn drop(&mut self) {
        unsafe {
            self.device.inner.device_wait_idle().unwrap();

            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None);
        }
    }
}

pub struct Swapchain {
    // device data
    device: Arc<DeviceHandles>,
    heap: Arc<RwLock<DescriptorHeap>>,
    // swapchain data
    inner: SwapchainHandle,
    // surface associated with this swapchain
    surface: Surface,
    info: SwapchainInfo,
    resources: PresentationResources,
    frame_index: u64,
    present_queue: vk::Queue,
}

impl SwapchainRHI for Swapchain {
    fn acquire_next_image(&mut self) -> Result<GpuPtr, Error> {
        let frame_index = self.frame_index;
        let frame_idx = frame_index as usize % self.resources.maximum_frames_in_flight();

        let acquire_semaphore = self.resources.acquire_semaphores[frame_idx];

        unsafe {
            let acquire_info = vk::AcquireNextImageInfoKHR::default()
                .device_mask(1)
                .swapchain(self.inner.swapchain)
                .timeout(u64::MAX)
                .semaphore(acquire_semaphore);

            let (image_idx, _) = match self
                .inner
                .swapchain_loader
                .acquire_next_image2(&acquire_info)
            {
                Ok(res) => res,
                Err(_) => {
                    let _ = self.recreate()?;
                    return Err(Error::SwapchainOutOfDate);
                }
            };

            let submit_semaphore = self.resources.submit_semaphores[image_idx as usize];

            let heap = self.heap.read().unwrap();

            let swapchain_image_ptr = self.resources.image_handles[image_idx as usize];

            let swapchain_image = heap.ptr_to_image(swapchain_image_ptr);

            assert!(
                swapchain_image.is_swapchain_image(),
                "Descriptor heap is corrupt!"
            );

            if let ImageData::Swapchain(data) = &swapchain_image.data {
                // Epic racy tearing writes
                data.submit_signal_present_wait.set(submit_semaphore);
                data.submit_wait.set(acquire_semaphore);
            }

            self.frame_index += 1;

            Ok(swapchain_image_ptr)
        }
    }

    fn present(&mut self, swapchain_image: crate::GpuPtr) -> Result<(), Error> {
        let heap = self.heap.read().unwrap();

        let swapchain_image = heap.ptr_to_image(swapchain_image);

        assert!(
            swapchain_image.is_swapchain_image(),
            "Descriptor heap is corrupt!"
        );

        let result = if let ImageData::Swapchain(data) = &swapchain_image.data {
            let queue = self.present_queue;

            let swapchains = [self.inner.swapchain];
            let wait_semaphores = [data.submit_signal_present_wait.get()];
            let indices = [data.idx];
            let present_info = vk::PresentInfoKHR::default()
                .swapchains(&swapchains)
                .wait_semaphores(&wait_semaphores)
                .image_indices(&indices);
            unsafe {
                self.inner
                    .swapchain_loader
                    .queue_present(queue, &present_info)
            }
        } else {
            unreachable!()
        };
        drop(heap);

        if result.is_err() {
            self.recreate().inspect_err(|_| {})?; // if not ready, we will just try again next time
        }
        Ok(())
    }
}

pub struct PresentationResources {
    acquire_semaphores: Vec<vk::Semaphore>, // frames_in_flight
    submit_semaphores: Vec<vk::Semaphore>,  // swapchain_size
    image_handles: Vec<GpuPtr>,
}

impl PresentationResources {
    fn new() -> Self {
        PresentationResources {
            acquire_semaphores: vec![],
            submit_semaphores: vec![],
            image_handles: vec![],
        }
    }
}

impl PresentationResources {
    fn maximum_frames_in_flight(&self) -> usize {
        self.acquire_semaphores.len()
    }

    fn swapchain_size(&self) -> usize {
        self.image_handles.len()
    }
}

impl Swapchain {
    pub(crate) unsafe fn new(
        device: Arc<DeviceHandles>,
        heap: Arc<RwLock<DescriptorHeap>>,
        window: &WindowSystemData,
        present_queue: vk::Queue,
        info: &SwapchainInfo,
    ) -> Result<Swapchain, vk::Result> {
        let WindowSystemData {
            display_handle,
            window_handle,
        } = window;

        unsafe {
            let surface = device
                .instance
                .create_surface(*display_handle, *window_handle);

            Self::create_swapchain(
                device,
                surface,
                heap,
                vk::SwapchainKHR::null(),
                present_queue,
                info,
            )
        }
    }

    pub(crate) fn recreate(&mut self) -> VkResult<()> {
        // swapchain lost due to surface caps becoming outdated
        let surface_caps = unsafe {
            self.surface
                .surface_loader
                .get_physical_device_surface_capabilities(self.device.pdevice, self.surface.inner)?
        };

        let swapchain = Self::create_or_recreate_swapchain(
            &self.inner.swapchain_loader,
            Arc::clone(&self.device),
            &self.surface,
            self.inner.swapchain,
            &self.info,
            surface_caps,
        )?;
        self.inner = swapchain;
        self.recreate_resources(&surface_caps)
    }

    fn create_or_recreate_swapchain(
        swapchain_loader: &khr::swapchain::Device,
        device: Arc<DeviceHandles>,
        surface: &Surface,
        old_swapchain: vk::SwapchainKHR,
        info: &SwapchainInfo,
        surface_caps: vk::SurfaceCapabilitiesKHR,
    ) -> Result<SwapchainHandle, vk::Result> {
        let surface_loader = &surface.surface_loader;

        if surface_caps.current_extent.height == 0 || surface_caps.current_extent.width == 0 {
            return Err(vk::Result::NOT_READY);
        }

        //let surface_format = Self::choose_surface_format(&device, surface_loader, &surface)?;

        let present_modes = unsafe {
            surface_loader
                .get_physical_device_surface_present_modes(device.pdevice, surface.inner)?
        };

        let present_mode = if present_modes.contains(&vk::PresentModeKHR::MAILBOX) {
            vk::PresentModeKHR::MAILBOX
        } else {
            vk::PresentModeKHR::FIFO
        };

        let swapchain_create_info = vk::SwapchainCreateInfoKHR::default()
            .surface(surface.inner)
            .image_extent(surface_caps.current_extent)
            .image_format(info.format)
            .image_color_space(info.color_space)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(surface_caps.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE) // PRE_MULTIPLIED is funky
            .image_array_layers(1)
            .min_image_count(info.size + 1)
            .present_mode(present_mode)
            .clipped(true)
            .old_swapchain(old_swapchain);

        let new_swapchain =
            unsafe { swapchain_loader.create_swapchain(&swapchain_create_info, None) };

        new_swapchain.map(|swapchain| SwapchainHandle {
            device,
            swapchain,
            swapchain_loader: swapchain_loader.clone(),
        })
    }

    unsafe fn create_swapchain(
        device: Arc<DeviceHandles>,
        surface: Surface,
        heap: Arc<RwLock<DescriptorHeap>>,
        swapchain: vk::SwapchainKHR,
        present_queue: vk::Queue,
        info: &SwapchainInfo,
    ) -> Result<Swapchain, vk::Result> {
        let swapchain_loader =
            khr::swapchain::Device::load(&device.instance.instance, &device.inner);

        let surface_caps = unsafe {
            surface
                .surface_loader
                .get_physical_device_surface_capabilities(device.pdevice, surface.inner)?
        };

        let swapchain = Self::create_or_recreate_swapchain(
            &swapchain_loader,
            Arc::clone(&device),
            &surface,
            swapchain,
            info,
            surface_caps,
        )?;

        let mut swapchain = Swapchain {
            device,
            heap,
            surface,
            inner: swapchain,
            resources: PresentationResources::new(),
            frame_index: 0,
            info: *info,
            present_queue,
        };

        swapchain.recreate_resources(&surface_caps)?;

        Ok(swapchain)
    }

    fn recreate_resources(
        &mut self,
        surface_caps: &vk::SurfaceCapabilitiesKHR,
    ) -> Result<(), vk::Result> {
        self.destroy_resources();
        unsafe {
            let device = &self.device;
            let mut heap = self.heap.write().unwrap();

            let swapchain_images = self
                .inner
                .swapchain_loader
                .get_swapchain_images(self.inner.swapchain)?;

            let swapchain_images = swapchain_images
                .into_iter()
                .enumerate()
                .map(|(idx, image)| {
                    let view = device.inner.create_image_view(
                        &vk::ImageViewCreateInfo::default()
                            .view_type(vk::ImageViewType::TYPE_2D)
                            .format(self.info.format)
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
                    let dimensions = [
                        surface_caps.current_extent.width,
                        surface_caps.current_extent.height,
                        1,
                    ];
                    Ok(Image {
                        inner: image,
                        view: Some(view),
                        desc: ImageDesc {
                            ty: vk::ImageType::TYPE_2D,
                            dimensions,
                            mip_count: 1,
                            layer_count: 1,
                            sample_count: 1,
                            format: self.info.format,
                            usage: crate::ImageUsage::Attachment,
                        },
                        current_layout: Cell::new(vk::ImageLayout::UNDEFINED),
                        data: ImageData::Swapchain(crate::device::SwapchainImageData {
                            idx: idx as u32,
                            submit_wait: Cell::new(vk::Semaphore::null()),
                            submit_signal_present_wait: Cell::new(vk::Semaphore::null()),
                        }),
                    })
                })
                .collect::<VkResult<Vec<Image>>>()?;

            for (idx, image) in swapchain_images.iter().enumerate() {
                let name = CString::new(format!("Swapchain Image {idx}")).unwrap();
                device.set_object_name(image.inner, &name);
            }

            let acquire_semaphores = (0..self.info.size)
                .map(|_| {
                    device
                        .inner
                        .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                })
                .collect::<VkResult<Vec<_>>>()?;
            let submit_semaphores = (0..(self.info.size + 1))
                .map(|_| {
                    device
                        .inner
                        .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                })
                .collect::<VkResult<Vec<_>>>()?;

            for (idx, semaphore) in acquire_semaphores.iter().enumerate() {
                let name = CString::new(format!("Swapchain acquire semaphore {idx}")).unwrap();
                device.set_object_name(*semaphore, &name);
            }
            for (idx, semaphore) in submit_semaphores.iter().enumerate() {
                let name = CString::new(format!("Swapchain submit semaphore {idx}")).unwrap();
                device.set_object_name(*semaphore, &name);
            }

            let image_handles = swapchain_images
                .into_iter()
                .map(|image| heap.insert_swapchain_image(image))
                .collect();

            let resources = PresentationResources {
                acquire_semaphores,
                submit_semaphores,
                image_handles,
            };

            self.resources = resources;
        }

        Ok(())
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

    fn destroy_resources(&mut self) {
        unsafe {
            self.device.inner.device_wait_idle().unwrap();

            for semaphore in &self.resources.acquire_semaphores {
                self.device.inner.destroy_semaphore(*semaphore, None);
            }

            for semaphore in &self.resources.submit_semaphores {
                self.device.inner.destroy_semaphore(*semaphore, None);
            }

            if !self.resources.image_handles.is_empty() {
                let mut heap = self.heap.write().unwrap();
                for ptr in &self.resources.image_handles {
                    heap.free_infallible(*ptr);
                }
            }
        }
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        self.destroy_resources();
    }
}
