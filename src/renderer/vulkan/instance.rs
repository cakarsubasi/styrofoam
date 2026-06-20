use std::ffi::c_char;
use std::mem::ManuallyDrop;
use std::sync::Arc;

use crate::renderer::vulkan::device::DeviceExtensions;
use crate::renderer::vulkan::device::ExtDescriptorHeap;

use super::*;

use ash::ext;
use ash::khr;
use ash::vk;
use ash::vk::TaggedStructure as _;
use winit::raw_window_handle::RawDisplayHandle;
use winit::raw_window_handle::RawWindowHandle;

use super::debug::create_debug_messenger;

pub struct Instance {
    entry: ManuallyDrop<ash::Entry>,
    pub(super) instance: ash::Instance,

    debug_utils_loader: ext::debug_utils::Instance,
    debug_messenger: vk::DebugUtilsMessengerEXT,

    pub(super) surface_loader: khr::surface::Instance,
}

type QueueFamilyIndex = u32;

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

    // TODO: maybe move device creation to device.rs instead?
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

        let (pdevice, queue_family_index) = self.choose_physical_device(surface);

        // Check if VK_EXT_descriptor_heap is supported
        let descriptor_heap_props = self.get_descriptor_heap_properties(&pdevice);

        // TODO: add extensions we want
        let enabled_extension_names = [
            // I like the theoretical possibility of running headless, but I am never going
            // to run this on an actually headless platform
            khr::swapchain::NAME.as_ptr(),
            khr::synchronization2::NAME.as_ptr(),
            // ext::device_fault::NAME.as_ptr(), // device errors
            // ext::shader_object::NAME.as_ptr(), // replace pipelines with shader objects
            ext::descriptor_heap::NAME.as_ptr(), // replace descriptor indexing with heaps
            // ext::mesh_shader::NAME.as_ptr(), // mesh shaders, weeee
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            ash::khr::portability_subset::NAME.as_ptr(),
        ];

        let vk10_features = vk::PhysicalDeviceFeatures::default().pipeline_statistics_query(true);
        let mut vk11_features =
            vk::PhysicalDeviceVulkan11Features::default().shader_draw_parameters(true);
        let mut vk12_features =
            vk::PhysicalDeviceVulkan12Features::default().buffer_device_address(true);
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
            .create_device(pdevice, &device_create_info, None)
            .expect("Failed to create device");

        let mut allocator_create_info =
            vk_mem::AllocatorCreateInfo::new(&self.instance, &device, pdevice);
        allocator_create_info.flags = vk_mem::AllocatorCreateFlags::BUFFER_DEVICE_ADDRESS;
        let allocator =
            vk_mem::Allocator::new(allocator_create_info).expect("Failed to create allocator");

        let queue_info = vk::DeviceQueueInfo2::default()
            .queue_family_index(queue_family_index)
            .queue_index(0);

        let queue = device.get_device_queue2(&queue_info);
        let debug_utils_loader = ext::debug_utils::Device::load(&self.instance, &device);

        let descriptor_heap = descriptor_heap_props.map(|props| ExtDescriptorHeap {
            device: ext::descriptor_heap::Device::load(&self.instance, &device),
            props,
        });

        Device {
            instance: self,
            physical_device: pdevice,
            inner: device,
            queue,
            queue_family_index,
            allocator,
            debug_utils_loader,
            ext: DeviceExtensions {
                descriptor_heap: descriptor_heap,
            },
        }
    }

    fn choose_physical_device(&self, surface: &Surface) -> (vk::PhysicalDevice, QueueFamilyIndex) {
        unsafe {
            let result = self
                .instance
                .enumerate_physical_devices()
                .unwrap()
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
                                Some((*pdevice, index as u32))
                            } else {
                                None
                            }
                        })
                })
                .expect("Couldn't find suitable device.");

            result
        }
    }

    fn get_descriptor_heap_properties(
        &self,
        pdevice: &vk::PhysicalDevice,
    ) -> Option<DescriptorHeapProps> {
        unsafe {
            let mut heap_features = vk::PhysicalDeviceDescriptorHeapFeaturesEXT::default();
            let mut features = vk::PhysicalDeviceFeatures2::default().push(&mut heap_features);

            self.instance
                .get_physical_device_features2(*pdevice, &mut features);

            if heap_features.descriptor_heap == vk::FALSE {
                return None;
            }

            let mut heap_properties = vk::PhysicalDeviceDescriptorHeapPropertiesEXT::default();
            let mut properties =
                vk::PhysicalDeviceProperties2::default().push(&mut heap_properties);

            self.instance
                .get_physical_device_properties2(*pdevice, &mut properties);

            Some(DescriptorHeapProps {
                sampler_descriptor_size: heap_properties.sampler_descriptor_size,
                image_descriptor_size: heap_properties.image_descriptor_size,
                buffer_descriptor_size: heap_properties.buffer_descriptor_size,
                sampler_heap_alignment: heap_properties.sampler_heap_alignment,
                resource_heap_alignment: heap_properties.resource_heap_alignment,
                min_sampler_heap_reserved_range: heap_properties.min_sampler_heap_reserved_range,
                min_resource_heap_reserved_range: heap_properties.min_resource_heap_reserved_range,
                max_resource_heap_size: heap_properties.max_resource_heap_size,
                max_sampler_heap_size: heap_properties.max_sampler_heap_size,
                max_push_data_size: heap_properties.max_push_data_size,
            })
        }
    }
}

#[derive(Debug, Clone)]
pub struct DescriptorHeapProps {
    pub sampler_descriptor_size: u64,
    pub image_descriptor_size: u64,
    pub buffer_descriptor_size: u64,
    pub sampler_heap_alignment: u64,
    pub resource_heap_alignment: u64,
    pub min_sampler_heap_reserved_range: u64,
    pub min_resource_heap_reserved_range: u64,
    pub max_resource_heap_size: u64,
    pub max_sampler_heap_size: u64,
    pub max_push_data_size: u64,
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
