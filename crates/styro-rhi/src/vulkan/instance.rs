use std::ffi::c_char;
use std::mem::ManuallyDrop;

use ash::ext;
use ash::khr;
use ash::vk;
use ash::vk::TaggedStructure as _;
use raw_window_handle::RawDisplayHandle;
use raw_window_handle::RawWindowHandle;

use crate::WindowSystem;
use crate::instance::DeviceCreateInfo;
use crate::instance::DeviceFeatures;
use crate::instance::PhysicalDeviceInfo;

use super::debug::create_debug_messenger;
use super::swapchain::Surface;

pub(crate) struct Instance {
    entry: ManuallyDrop<ash::Entry>,
    pub(super) instance: ash::Instance,

    debug_utils_loader: ext::debug_utils::Instance,
    debug_messenger: vk::DebugUtilsMessengerEXT,

    pub(super) _surface_loader: khr::surface::Instance,
}

type QueueFamilyIndex = u32;

pub(super) struct DeviceResult {
    pub device: ash::Device,
    pub pdevice: vk::PhysicalDevice,
    pub _graphics_queue_index: QueueFamilyIndex,
    pub _compute_queue_index: QueueFamilyIndex,
    pub _transfer_queue_index: QueueFamilyIndex,
}

pub fn enumerate_required_windowing_extensions(
    display_handle: WindowSystem,
) -> ash::VkResult<&'static [*const c_char]> {
    let extensions = match display_handle {
        WindowSystem::Headless => {
            const HEADLESS_EXTS: [*const c_char; 2] = [
                khr::surface::NAME.as_ptr(),
                ext::headless_surface::NAME.as_ptr(),
            ];
            &HEADLESS_EXTS
        }

        WindowSystem::Windows => {
            const WINDOWS_EXTS: [*const c_char; 2] = [
                khr::surface::NAME.as_ptr(),
                khr::win32_surface::NAME.as_ptr(),
            ];
            &WINDOWS_EXTS
        }

        WindowSystem::Wayland => {
            const WAYLAND_EXTS: [*const c_char; 2] = [
                khr::surface::NAME.as_ptr(),
                khr::wayland_surface::NAME.as_ptr(),
            ];
            &WAYLAND_EXTS
        }

        WindowSystem::Xlib => {
            const XLIB_EXTS: [*const c_char; 2] = [
                khr::surface::NAME.as_ptr(),
                khr::xlib_surface::NAME.as_ptr(),
            ];
            &XLIB_EXTS
        }

        WindowSystem::Xcb => {
            const XCB_EXTS: [*const c_char; 2] =
                [khr::surface::NAME.as_ptr(), khr::xcb_surface::NAME.as_ptr()];
            &XCB_EXTS
        }

        // Maybe in a decade
        //WindowingSystem::Android => {
        //    const ANDROID_EXTS: [*const c_char; 2] = [
        //        khr::surface::NAME.as_ptr(),
        //        khr::android_surface::NAME.as_ptr(),
        //    ];
        //    &ANDROID_EXTS
        //}
        WindowSystem::AppKit | WindowSystem::UiKit => {
            const METAL_EXTS: [*const c_char; 2] = [
                khr::surface::NAME.as_ptr(),
                ext::metal_surface::NAME.as_ptr(),
            ];
            &METAL_EXTS
        }

        _ => return Err(vk::Result::ERROR_EXTENSION_NOT_PRESENT),
    };

    Ok(extensions)
}

impl Instance {
    pub(crate) fn enumerate_physical_devices(&self) -> Vec<PhysicalDeviceInfo> {
        unsafe {
            let pdevices = self
                .instance
                .enumerate_physical_devices()
                .expect("Physical device error");

            pdevices
                .into_iter()
                .enumerate()
                .flat_map(|(idx, pdevice)| {
                    let mut properties = Default::default();
                    self.instance
                        .get_physical_device_properties2(pdevice, &mut properties);

                    let device_name: String = properties
                        .properties
                        .device_name_as_c_str()
                        .unwrap_or(c"")
                        .to_string_lossy()
                        .into_owned();

                    let mut descriptor_heap_feature =
                        vk::PhysicalDeviceDescriptorHeapFeaturesEXT::default();
                    let mut shader_untyped_ptrs_feature =
                        vk::PhysicalDeviceShaderUntypedPointersFeaturesKHR::default();
                    let mut extended_dynamic_state3_feature =
                        vk::PhysicalDeviceExtendedDynamicState3FeaturesEXT::default();
                    let mut features = vk::PhysicalDeviceFeatures2::default()
                        .push(&mut descriptor_heap_feature)
                        .push(&mut shader_untyped_ptrs_feature)
                        .push(&mut extended_dynamic_state3_feature);
                    self.instance
                        .get_physical_device_features2(pdevice, &mut features);

                    let extensions = self
                        .instance
                        .enumerate_device_extension_properties(pdevice)
                        .ok()?;

                    let push_descriptor = extensions
                        .iter()
                        .find(|&extension| {
                            extension
                                .extension_name_as_c_str()
                                .is_ok_and(|name| name == khr::push_descriptor::NAME)
                        })
                        .is_some();

                    let mesh_shader = extensions
                        .iter()
                        .find(|&extension| {
                            extension
                                .extension_name_as_c_str()
                                .is_ok_and(|name| name == ext::mesh_shader::NAME)
                        })
                        .is_some();

                    let device_features = DeviceFeatures {
                        descriptor_heap: descriptor_heap_feature.descriptor_heap != 0
                            && shader_untyped_ptrs_feature.shader_untyped_pointers != 0,
                        dynamic_blend_state: extended_dynamic_state3_feature
                            .extended_dynamic_state3_color_blend_enable
                            != 0
                            && extended_dynamic_state3_feature
                                .extended_dynamic_state3_color_blend_equation
                                != 0
                            && extended_dynamic_state3_feature
                                .extended_dynamic_state3_color_write_mask
                                != 0,
                        push_descriptor,
                        mesh_shader,
                    };

                    Some(PhysicalDeviceInfo {
                        device_idx: idx,
                        device_name,
                        device_features,
                    })
                })
                .collect()
        }
    }
}

fn load_vulkan_entry() -> ash::Entry {
    let err = match unsafe { ash::Entry::load() } {
        Ok(entry) => return entry,
        Err(err) => err,
    };

    // We shall just assume you are reasonable enough to have Vulkan installed on the system
    #[cfg(target_os = "macos")]
    {
        let mut candidates = Vec::new();
        if let Some(sdk) = std::env::var_os("VULKAN_SDK") {
            candidates.push(std::path::PathBuf::from(sdk).join("lib/libvulkan.dylib"));
        }
        candidates.push("/usr/local/lib/libvulkan.dylib".into());
        candidates.push("/opt/homebrew/lib/libvulkan.dylib".into());

        for path in candidates {
            if let Ok(entry) = unsafe { ash::Entry::load_from(&path) } {
                return entry;
            }
        }
    }
    panic!("Failed to load Vulkan: {err:?}");
}

impl Instance {
    pub fn new() -> Self {
        Self::new_with_presentation(WindowSystem::None)
    }

    pub fn new_with_presentation(windowing_system: WindowSystem) -> Self {
        let entry = load_vulkan_entry();
        let app_name = c"Styro RHI";

        let mut extension_names = enumerate_required_windowing_extensions(windowing_system)
            .map_or(vec![], |slice| slice.to_vec());
        extension_names.push(ext::debug_utils::NAME.as_ptr());
        extension_names.push(khr::surface_maintenance1::NAME.as_ptr());
        extension_names.push(khr::get_surface_capabilities2::NAME.as_ptr());

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

        // There is no MVK support whatsoever right now due to the lack of
        // necessary extensions but we should keep this just in case it is added
        let create_flags = if cfg!(any(target_os = "macos", target_os = "ios")) {
            vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR
        } else {
            vk::InstanceCreateFlags::default()
        };

        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&appinfo)
            //    .enabled_layer_names(&layers_names_raw)
            .enabled_extension_names(&extension_names)
            .flags(create_flags);

        unsafe {
            let instance: ash::Instance = entry
                .create_instance(&create_info, None)
                .expect("Instance creation error");

            let debug_utils_loader = ext::debug_utils::Instance::load(&entry, &instance);
            let debug_messenger = create_debug_messenger(&debug_utils_loader);

            let _surface_loader = ash::khr::surface::Instance::load(&entry, &instance);

            Self {
                entry: ManuallyDrop::new(entry),
                instance,

                debug_utils_loader,
                debug_messenger,

                _surface_loader,
            }
        }
    }

    pub(super) unsafe fn create_surface(
        &self,
        raw_display_handle: RawDisplayHandle,
        raw_window_handle: RawWindowHandle,
    ) -> Surface {
        unsafe {
            let surface =
                ash_window::SurfaceFactory::new(&self.entry, &self.instance, raw_display_handle)
                    .unwrap()
                    .create_surface(raw_window_handle, None)
                    .unwrap();

            let surface_loader = khr::surface::Instance::load(&self.entry, &self.instance);

            Surface {
                inner: surface,
                surface_loader,
            }
        }
    }

    // TODO: maybe move device creation to device.rs instead?
    pub(super) unsafe fn create_device(&self, device_info: &DeviceCreateInfo) -> DeviceResult {
        let pdevices = unsafe {
            self.instance
                .enumerate_physical_devices()
                .expect("Physical device error")
        };

        // TODO
        let pdevice = *pdevices.get(device_info.device_idx).unwrap_or_else(|| {
            panic!(
                "Device index {} is out of bounds. There are only {} devices available.",
                device_info.device_idx,
                pdevices.len()
            )
        });

        let device_features = device_info.device_features;

        let (graphics_queue, compute_queue, transfer_queue) = self.choose_physical_device(pdevice);

        let mut enabled_extension_names = vec![
            // I like the theoretical possibility of running headless, but I am never going
            // to run this on an actually headless platform
            khr::swapchain::NAME.as_ptr(),
            khr::swapchain_maintenance1::NAME.as_ptr(),
            khr::synchronization2::NAME.as_ptr(),
            khr::dynamic_rendering_local_read::NAME.as_ptr(),
            khr::push_descriptor::NAME.as_ptr(),
            khr::maintenance5::NAME.as_ptr(),
            // ext::device_fault::NAME.as_ptr(), // device errors
            //khr::device_address_commands::NAME.as_ptr(), // Replaces commands taking buffers with those taking pointers
            // Probably won't use this because of such narrow support
            // khr::unified_image_layouts::NAME.as_ptr(), // Eliminates need for most layout transitions. Poor support outside Nvidia.
            // ext::mesh_shader::NAME.as_ptr(), // mesh shaders, weeee
        ];
        if device_features.descriptor_heap {
            enabled_extension_names.push(ext::descriptor_heap::NAME.as_ptr()); // replace descriptor indexing with heaps
            enabled_extension_names.push(khr::shader_untyped_pointers::NAME.as_ptr()) // Accessing shader resources without providing types
        }
        if device_features.dynamic_blend_state {
            enabled_extension_names.push(ext::extended_dynamic_state3::NAME.as_ptr()); // Bake fewer things into the PSO
        }
        if device_features.mesh_shader {
            enabled_extension_names.push(ext::mesh_shader::NAME.as_ptr());
        }

        let vk10_features = vk::PhysicalDeviceFeatures::default()
            .pipeline_statistics_query(false)
            .depth_bias_clamp(true)
            .dual_src_blend(true)
            .sampler_anisotropy(true);
        let mut vk11_features =
            vk::PhysicalDeviceVulkan11Features::default().shader_draw_parameters(true);
        let mut vk12_features = vk::PhysicalDeviceVulkan12Features::default()
            .buffer_device_address(true)
            .timeline_semaphore(true);
        let mut vk13_features = vk::PhysicalDeviceVulkan13Features::default()
            .dynamic_rendering(true)
            .synchronization2(true);
        //let mut vk14_features = vk::PhysicalDeviceVulkan14Features::default()
        //    .dynamic_rendering_local_read(true) // subpasses
        //    .maintenance5(true); // pass spirv directly rather than creating a ShaderModule
        let mut maintenance5_features =
            vk::PhysicalDeviceMaintenance5Features::default().maintenance5(true);
        let mut descriptor_heap_feature = vk::PhysicalDeviceDescriptorHeapFeaturesEXT::default()
            .descriptor_heap(device_features.descriptor_heap)
            .descriptor_heap_capture_replay(false); // Nvidia 595.79 - capture replay is not supported
        let mut shader_untyped_ptrs_feature =
            vk::PhysicalDeviceShaderUntypedPointersFeaturesKHR::default()
                .shader_untyped_pointers(device_features.descriptor_heap);
        let mut extended_dynamic_state3_feature =
            vk::PhysicalDeviceExtendedDynamicState3FeaturesEXT::default()
                .extended_dynamic_state3_color_blend_enable(device_features.dynamic_blend_state)
                .extended_dynamic_state3_color_blend_equation(device_features.dynamic_blend_state)
                .extended_dynamic_state3_color_write_mask(device_features.dynamic_blend_state);
        let mut dynamic_rendering_local_read_feature =
            vk::PhysicalDeviceDynamicRenderingLocalReadFeatures::default()
                .dynamic_rendering_local_read(true);

        let mut enabled_features = vk::PhysicalDeviceFeatures2::default()
            .features(vk10_features)
            .push(&mut vk11_features)
            .push(&mut vk12_features)
            .push(&mut vk13_features)
            .push(&mut maintenance5_features)
            //.push(&mut vk14_features)
            .push(&mut descriptor_heap_feature)
            .push(&mut shader_untyped_ptrs_feature)
            .push(&mut extended_dynamic_state3_feature)
            .push(&mut dynamic_rendering_local_read_feature);

        let queue_create_infos = [
            vk::DeviceQueueCreateInfo::default()
                .queue_family_index(graphics_queue)
                .queue_priorities(&[0.5]),
            //vk::DeviceQueueCreateInfo::default()
            //    .queue_family_index(compute_queue)
            //    .queue_priorities(&[0.5]),
            //vk::DeviceQueueCreateInfo::default()
            //    .queue_family_index(transfer_queue)
            //    .queue_priorities(&[0.5]),
        ];

        unsafe {
            let device_create_info = vk::DeviceCreateInfo::default()
                .enabled_extension_names(&enabled_extension_names)
                .queue_create_infos(&queue_create_infos)
                .extend(&mut enabled_features);

            let device = self
                .instance
                .create_device(pdevice, &device_create_info, None)
                .expect("Failed to create device");

            DeviceResult {
                device,
                pdevice,
                _graphics_queue_index: graphics_queue,
                _compute_queue_index: compute_queue,
                _transfer_queue_index: transfer_queue,
            }
        }
    }

    fn choose_physical_device(
        &self,
        pdevice: vk::PhysicalDevice,
    ) -> (QueueFamilyIndex, QueueFamilyIndex, QueueFamilyIndex) {
        unsafe {
            // Then figure out the queues
            let queue_family_props = self
                .instance
                .get_physical_device_queue_family_properties(pdevice);

            // graphics queue should support presentation, the other queues do not matter
            let graphics_id = queue_family_props
                .iter()
                .enumerate()
                .find(|&(_idx, props)| {
                    props.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                        && props.queue_flags.contains(vk::QueueFlags::COMPUTE)
                        && props.queue_flags.contains(vk::QueueFlags::TRANSFER)
                })
                .unwrap()
                .0 as u32;

            //let compute_id = queue_family_props
            //    .iter()
            //    .enumerate()
            //    .find(|&(_idx, props)| {
            //        !props.queue_flags.contains(vk::QueueFlags::GRAPHICS)
            //            && props.queue_flags.contains(vk::QueueFlags::COMPUTE)
            //            && props.queue_flags.contains(vk::QueueFlags::TRANSFER)
            //    })
            //    .unwrap()
            //    .0 as u32;

            //let transfer_id = queue_family_props
            //    .iter()
            //    .enumerate()
            //    .find(|&(_idx, props)| {
            //        !props.queue_flags.contains(vk::QueueFlags::GRAPHICS)
            //            && !props.queue_flags.contains(vk::QueueFlags::COMPUTE)
            //            && props.queue_flags.contains(vk::QueueFlags::TRANSFER)
            //    })
            //    .unwrap()
            //    .0 as u32;

            (graphics_id, 0, 0)
        }
    }

    pub fn get_descriptor_heap_properties(
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

            // Due to the PhantomData field in vk::PhysicalDeviceDescriptorHeapPropertiesEXT, we need our own struct
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
