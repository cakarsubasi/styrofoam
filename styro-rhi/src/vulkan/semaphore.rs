use super::device::DeviceHandles;
use ash::vk;
use std::ffi::CStr;
use std::sync::Arc;

#[repr(C)]
pub struct Semaphore {
    pub(super) device: Arc<DeviceHandles>,
    pub(super) inner: vk::Semaphore,
}

impl Semaphore {
    pub fn wait(&self, value: u64) {
        let semaphores = [self.inner];
        let values = [value];

        let wait_info = vk::SemaphoreWaitInfo::default()
            .semaphores(&semaphores)
            .values(&values);

        unsafe {
            self.device
                .inner
                .wait_semaphores(&wait_info, u64::MAX)
                .unwrap()
        };
    }

    pub fn get_value(&self) -> u64 {
        unsafe {
            self.device
                .inner
                .get_semaphore_counter_value(self.inner)
                .unwrap()
        }
    }

    pub fn set_object_name(&self, name: &CStr) {
        self.device.set_object_name(self.inner, name);
    }
}

impl Drop for Semaphore {
    fn drop(&mut self) {
        unsafe {
            self.device.inner.destroy_semaphore(self.inner, None);
        }
    }
}
