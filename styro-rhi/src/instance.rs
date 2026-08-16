//!
//! Multiple graphics adapters may be available on modern hardware. The instance
//!
//!
use super::vulkan::instance::Instance;

use crate::{Device, WindowSystem};

/// Structure describing physical devices
#[derive(Debug, Clone)]
pub struct PhysicalDeviceInfo {
    /// The index of the physical device
    pub device_idx: usize,
    /// The physical device name
    pub device_name: String,
    /// The physical device features
    pub device_features: DeviceFeatures,
}

/// Enumerate physical devices available on the system
///
/// # Valid usage
/// Infallible
pub fn enumerate_physical_devices() -> Vec<PhysicalDeviceInfo> {
    let instance = Instance::new();

    instance.enumerate_physical_devices()
}

/// Physical device features that are supported by an implementation
#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceFeatures {
    /// Enables explicit descriptor management using [crate::DeviceRHI::get_image_descriptor],
    /// [crate::DeviceRHI::get_sampler_descriptor], and [crate::CommandRHI::bind_descriptor_heap].
    ///
    /// On Vulkan, corresponds to VK_EXT_descriptor_heap and VK_KHR_shader_untyped_pointers
    pub descriptor_heap: bool,
    /// Enable "vanilla bindless" using [crate::CommandRHI::push_descriptors]. This option is simpler
    /// and more widely supported than `descriptor_heap` but is limited to 32 combined image samplers
    /// per shader.
    ///
    /// On Vulkan, corresponds to VK_KHR_push_descriptor.
    pub push_descriptor: bool,
    /// Enables using [crate::CommandRHI::set_blend_state] instead of baking
    /// blend state inside a [crate::Pipeline].
    ///
    /// On Vulkan, corresponds to VK_EXT_extended_dynamic_state3
    pub dynamic_blend_state: bool,
}

/// Parameters of a newly created device
pub struct DeviceCreateInfo {
    /// The index of the physical device
    pub device_idx: usize,
    /// The requested device features
    ///
    /// Enabled features must be supported based on the return value of
    /// [enumerate_physical_devices].
    pub device_features: DeviceFeatures,
    /// Requested window system.
    pub presentation: Option<WindowSystem>,
    // TODO: add a way to pass an external debug callback
    //pub debug_callback: Option<usize>,
}

/// Create a logical graphics adapter.
///
/// # Valid usage
/// - `info.device_idx` must be less than the number of physical devices.
/// - `info.device_features` must be supported by the physical device.
/// - If `info.presentation` is not `None`, the given window system must be in use.
pub fn create_device(info: &DeviceCreateInfo) -> super::Device {
    Device::new(info)
}
