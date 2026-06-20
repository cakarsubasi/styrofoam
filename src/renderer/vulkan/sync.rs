use super::*;
use ash::VkResult;
use ash::ext;
use ash::khr;
use ash::vk;

use std::sync::Arc;

pub struct Fence {
    pub(super) device: Arc<Device>,
    pub(super) inner: vk::Fence,
}

impl Fence {
    pub fn new(device: Arc<Device>) -> VkResult<Self> {
        device.create_fence()
    }

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

impl Drop for Fence {
    fn drop(&mut self) {
        unsafe {
            self.device.inner.destroy_fence(self.inner, None);
        }
    }
}

pub struct Semaphore {
    pub(super) device: Arc<Device>,
    pub(super) inner: vk::Semaphore,
}

impl Semaphore {
    pub fn new(device: Arc<Device>) -> VkResult<Self> {
        device.create_semaphore()
    }
}

impl Drop for Semaphore {
    fn drop(&mut self) {
        unsafe {
            self.device.inner.destroy_semaphore(self.inner, None);
        }
    }
}
