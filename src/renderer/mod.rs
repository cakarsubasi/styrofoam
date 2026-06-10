#![allow(unsafe_op_in_unsafe_fn)]

use ash;
use ash::vk;
use core::ffi;
use std::borrow::Cow;
use std::sync::Arc;
use winit::raw_window_handle::RawDisplayHandle;
use winit::raw_window_handle::RawWindowHandle;

unsafe extern "system" fn vulkan_debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user_data: *mut std::os::raw::c_void,
) -> vk::Bool32 {
    let callback_data = *p_callback_data;
    let message_id_number = callback_data.message_id_number;

    let message_id_name = if callback_data.p_message_id_name.is_null() {
        Cow::from("")
    } else {
        ffi::CStr::from_ptr(callback_data.p_message_id_name).to_string_lossy()
    };

    let message = if callback_data.p_message.is_null() {
        Cow::from("")
    } else {
        ffi::CStr::from_ptr(callback_data.p_message).to_string_lossy()
    };

    println!(
        "{message_severity:?}:\n{message_type:?} [{message_id_name} ({message_id_number})] : {message}\n",
    );

    vk::FALSE
}

pub mod vkhandles {
    use std::ffi::c_char;
    use std::mem::ManuallyDrop;
    use std::sync::Arc;

    use winit::raw_window_handle::RawDisplayHandle;
    use winit::raw_window_handle::RawWindowHandle;

    use super::*;

    use ash::ext;
    use ash::khr;

    pub struct Surface {
        instance: Arc<Instance>,
        surface: vk::SurfaceKHR,
        //surface_format: vk::SurfaceFormatKHR,
        //surface_resolution: vk::Extent2D,
        surface_loader: khr::surface::Instance,
    }

    impl Drop for Surface {
        fn drop(&mut self) {
            unsafe {
                self.instance
                    .surface_loader
                    .destroy_surface(self.surface, None);
            }
        }
    }

    pub struct Swapchain {
        instance: Arc<Device>,
        surface: Arc<Surface>,
        swapchain: vk::SwapchainKHR,
        swapchain_loader: khr::swapchain::Device,
    }

    impl Swapchain {
        pub unsafe fn new(
            device: Arc<Device>,
            surface: Arc<Surface>,
        ) -> Result<Swapchain, vk::Result> {
            Self::create_swapchain(device, surface, vk::SwapchainKHR::null())
        }

        unsafe fn create_swapchain(
            device: Arc<Device>,
            surface: Arc<Surface>,
            swapchain: vk::SwapchainKHR,
        ) -> Result<Swapchain, vk::Result> {
            const SWAPCHAIN_SIZE: u32 = 3;

            let swapchain_loader =
                khr::swapchain::Device::new(&device.instance.instance, &device.device);

            let surface_loader = &surface.surface_loader;

            let surface_caps = surface_loader.get_physical_device_surface_capabilities(
                device.physical_device,
                surface.surface,
            )?;

            let surface_formats = surface_loader
                .get_physical_device_surface_formats(device.physical_device, surface.surface)?;

            // TODO: should probably fetch an RGB format instead of BGR
            let surface_format = surface_formats[0];

            let present_modes = surface_loader.get_physical_device_surface_present_modes(
                device.physical_device,
                surface.surface,
            )?;

            let present_mode = if present_modes.contains(&vk::PresentModeKHR::MAILBOX) {
                vk::PresentModeKHR::MAILBOX
            } else {
                vk::PresentModeKHR::FIFO
            };

            let swapchain_create_info = vk::SwapchainCreateInfoKHR::default()
                .surface(surface.surface)
                .image_extent(surface_caps.current_extent)
                .image_format(surface_format.format)
                .image_color_space(surface_format.color_space)
                .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
                .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                .pre_transform(surface_caps.current_transform)
                .composite_alpha(vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED)
                .image_array_layers(1)
                .min_image_count(SWAPCHAIN_SIZE)
                .present_mode(present_mode)
                .clipped(true)
                .old_swapchain(swapchain);

            // TODO: should be able to handle this
            let swapchain = swapchain_loader
                .create_swapchain(&swapchain_create_info, None)
                .expect("Cannot create swapchain");

            Ok(Swapchain {
                instance: device,
                surface: surface,
                swapchain,
                swapchain_loader,
            })
        }
    }

    impl Drop for Swapchain {
        fn drop(&mut self) {
            unsafe {
                self.swapchain_loader
                    .destroy_swapchain(self.swapchain, None);
            }
        }
    }

    pub struct Queue {
        instance: Arc<Instance>,
        queue: vk::Queue,
    }

    impl Drop for Queue {
        fn drop(&mut self) {
            unsafe {
                //self.instance.instance.
            }
        }
    }

    pub struct Device {
        instance: Arc<Instance>,
        physical_device: vk::PhysicalDevice,
        device: ash::Device,
        queue: vk::Queue,
        allocator: vk_mem::Allocator,
        //swapchain: vk::SwapchainKHR,
    }

    impl Device {}

    impl Drop for Device {
        fn drop(&mut self) {
            unsafe {
                self.device.destroy_device(None);
            }
        }
    }

    pub struct Instance {
        entry: ManuallyDrop<ash::Entry>,
        instance: ash::Instance,

        debug_utils_loader: ext::debug_utils::Instance,
        debug_messenger: vk::DebugUtilsMessengerEXT,

        surface_loader: khr::surface::Instance,

        swapchain_loader: khr::swapchain::Instance,
    }

    impl Instance {
        pub unsafe fn new(raw_display_handle: RawDisplayHandle) -> Self {
            let entry = ash::Entry::load().expect("Failed to load Vulkan");
            let app_name = c"VulkanTriangle";

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

            let debug_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
                .message_severity(
                    vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                        | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                        | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
                )
                .message_type(
                    vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                        | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                        | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                )
                .pfn_user_callback(Some(vulkan_debug_callback));

            let debug_utils_loader = ext::debug_utils::Instance::new(&entry, &instance);
            let debug_messenger = debug_utils_loader
                .create_debug_utils_messenger(&debug_info, None)
                .unwrap();

            let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);
            let swapchain_loader = khr::swapchain::Instance::new(&entry, &instance);

            Self {
                entry: ManuallyDrop::new(entry),
                instance,

                debug_utils_loader,
                debug_messenger,

                surface_loader,
                swapchain_loader,
            }
        }

        pub unsafe fn create_surface(
            self: Arc<Self>,
            raw_display_handle: RawDisplayHandle,
            raw_window_handle: RawWindowHandle,
        ) -> Surface {
            let surface = ash_window::create_surface(
                &self.entry,
                &self.instance,
                raw_display_handle,
                raw_window_handle,
                None,
            )
            .unwrap();

            let surface_loader = khr::surface::Instance::new(&self.entry, &self.instance);

            Surface {
                instance: self,
                surface,
                surface_loader,
            }
        }

        pub unsafe fn create_physical_device(self: Arc<Self>, surface: &Surface) -> Device {
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

            let (_pdevice, queue_family_index) = pdevices
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
                                            surface.surface,
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
            let _queue_family_index = queue_family_index as u32;

            // TODO: add extensions we want
            let enabled_extension_names = [
                khr::swapchain::NAME.as_ptr(),
                #[cfg(any(target_os = "macos", target_os = "ios"))]
                ash::khr::portability_subset::NAME.as_ptr(),
            ];

            let vk10_features =
                vk::PhysicalDeviceFeatures::default().pipeline_statistics_query(true);
            let mut vk11_features =
                vk::PhysicalDeviceVulkan11Features::default().shader_draw_parameters(true);
            let mut vk12_features = vk::PhysicalDeviceVulkan12Features::default();
            let mut vk13_features =
                vk::PhysicalDeviceVulkan13Features::default().dynamic_rendering(true);
            let mut enabled_features = vk::PhysicalDeviceFeatures2 {
                features: vk10_features,
                ..Default::default()
            }
            .push_next(&mut vk11_features)
            .push_next(&mut vk12_features)
            .push_next(&mut vk13_features);

            let queue_create_infos = [vk::DeviceQueueCreateInfo::default()
                .queue_family_index(_queue_family_index)
                .queue_priorities(&[1.0])];

            let device_create_info = vk::DeviceCreateInfo::default()
                .enabled_extension_names(&enabled_extension_names)
                .queue_create_infos(&queue_create_infos)
                .push_next(&mut enabled_features);

            let device = self
                .instance
                .create_device(_pdevice, &device_create_info, None)
                .expect("Failed to create device");

            let allocator_create_info =
                vk_mem::AllocatorCreateInfo::new(&self.instance, &device, _pdevice);

            let allocator =
                vk_mem::Allocator::new(allocator_create_info).expect("Failed to create allocator");

            let queue_info = vk::DeviceQueueInfo2::default()
                .queue_family_index(_queue_family_index)
                .queue_index(0);

            let queue = device.get_device_queue2(&queue_info);
            let swapchain_loader = khr::swapchain::Device::new(&self.instance, &device);

            Device {
                instance: self,
                physical_device: _pdevice,
                device,
                queue,
                allocator,
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
}

pub struct Renderer {
    device: Arc<vkhandles::Device>,
}
impl Renderer {
    pub unsafe fn new(
        raw_display_handle: RawDisplayHandle,
        raw_window_handle: RawWindowHandle,
    ) -> Self {
        let instance = Arc::new(vkhandles::Instance::new(raw_display_handle));
        let surface = Arc::clone(&instance).create_surface(raw_display_handle, raw_window_handle);
        let device = Arc::new(instance.create_physical_device(&surface));
        let swapchain = vkhandles::Swapchain::new(Arc::clone(&device), Arc::new(surface))
            .expect("Initial swapchain creation failure");
        Self { device: device }
    }
}
