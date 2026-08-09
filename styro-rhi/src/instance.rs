use super::vulkan::instance::Instance;

use crate::{Device, WindowingSystem};

#[derive(Debug, Clone)]
pub struct PhysicalDeviceInfo {
    pub device_idx: usize,
    pub device_name: String,
    pub device_features: DeviceFeatures,
}

pub fn enumerate_physical_devices() -> Vec<PhysicalDeviceInfo> {
    let instance = Instance::new();

    instance.enumerate_physical_devices()
}

#[derive(Debug, Clone, Copy)]
pub struct DeviceFeatures {
    pub descriptor_heap: bool,
    pub dynamic_blend_state: bool,
}

pub struct DeviceCreateInfo {
    pub device_idx: usize,
    pub device_features: DeviceFeatures,
    pub presentation: Option<WindowingSystem>,
    //pub debug_callback: Option<usize>,
}

pub fn create_device(info: &DeviceCreateInfo) -> super::Device {
    Device::new(info)
}
