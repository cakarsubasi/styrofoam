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
#[derive(Debug, Clone, Copy)]
pub struct DeviceFeatures {
    /// Enables explicit descriptor management using [DeviceRHI::get_image_descriptor],
    /// [DeviceRHI::get_sampler_descriptor], and [CommandRHI::bind_descriptor_heap].
    /// Required to be able to use images.
    // TODO: provide a push descriptor fallback
    pub descriptor_heap: bool,
    /// Enables using [CommandRHI::set_blend_state] instead of baking
    /// blend state inside a [Pipeline].
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
