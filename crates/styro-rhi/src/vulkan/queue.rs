use std::sync::Arc;
use std::sync::RwLock;
use std::sync::Weak;
use std::sync::atomic::AtomicI32;
use std::sync::atomic::Ordering;

use ash::vk;

use super::command::CommandBuffer;
use super::command::LayoutTransition;
use super::device::DescriptorHeap;
use super::device::DeviceHandles;

use crate::*;

struct CommandPool {
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    used: AtomicI32,
}

impl CommandPool {
    fn new(device: &ash::Device, queue_index: u32, command_buffer_count: u32) -> Self {
        unsafe {
            let command_pool_create_info = &vk::CommandPoolCreateInfo::default()
                //.flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(queue_index);

            let command_pool = device
                .create_command_pool(command_pool_create_info, None)
                .unwrap();

            let allocate_info = vk::CommandBufferAllocateInfo::default()
                .command_buffer_count(command_buffer_count)
                .command_pool(command_pool);

            let command_buffers = device.allocate_command_buffers(&allocate_info).unwrap();
            Self {
                command_pool,
                command_buffers,
                used: AtomicI32::new(0),
            }
        }
    }

    fn reset(&self, device: &ash::Device) {
        unsafe {
            device
                .reset_command_pool(
                    self.command_pool,
                    vk::CommandPoolResetFlags::RELEASE_RESOURCES,
                )
                .unwrap();

            self.used.store(0, Ordering::Release);
        }
    }

    unsafe fn destroy(&self, device: &ash::Device) {
        unsafe {
            device.destroy_command_pool(self.command_pool, None);
        }
    }
}

struct QueuePool {
    pub(crate) _family_index: u32,
    queue: vk::Queue,
    command_pools: Vec<CommandPool>,
}

impl QueuePool {
    fn new(
        device: &ash::Device,
        queue_index: u32,
        command_pools: u32,
        command_buffers_per_pool: u32,
    ) -> Self {
        unsafe {
            let queue = device.get_device_queue(queue_index, 0);

            let command_pools = (0..command_pools)
                .into_iter()
                .map(|_| CommandPool::new(device, queue_index, command_buffers_per_pool))
                .collect();

            Self {
                _family_index: queue_index,
                queue,
                command_pools: command_pools,
            }
        }
    }
}

pub struct Queue {
    device: Weak<DeviceHandles>,
    heap: Weak<RwLock<DescriptorHeap>>,
    queue: QueuePool,
}

impl Queue {
    pub(super) fn new(
        device: &Arc<DeviceHandles>,
        heap: &Arc<RwLock<DescriptorHeap>>,
        family_index: u32,
        command_pools: u32,
        command_buffers_per_pool: u32,
    ) -> Self {
        Self {
            device: Arc::downgrade(device),
            heap: Arc::downgrade(heap),
            queue: QueuePool::new(
                &device.inner,
                family_index,
                command_pools,
                command_buffers_per_pool,
            ),
        }
    }

    pub(super) fn queue(&self) -> vk::Queue {
        self.queue.queue
    }

    pub(super) fn handles(&self) -> Arc<DeviceHandles> {
        self.device.upgrade().unwrap()
    }

    fn get_command_buffer(&mut self, command_pool: u32) -> vk::CommandBuffer {
        let queue = &self.queue; //.upgrade().unwrap();
        let command_pool = queue
            .command_pools
            .get(command_pool as usize)
            .expect(&format!(
                "Invalid command pool index {}. There are only {} command pools available.",
                command_pool,
                queue.command_pools.len()
            ));

        if command_pool.used.load(Ordering::Acquire) == -1 {
            command_pool.reset(&self.device.upgrade().unwrap().inner);
        }

        let idx = command_pool.used.fetch_add(1, Ordering::Release);
        let command_buffer = command_pool.command_buffers.get(idx as usize).expect(&format!(
            "Attempted to requested {} command buffers. But this command pool only has {} available.",
            idx + 1,
            command_pool.command_buffers.len()));
        *command_buffer
    }

    fn submit_impl(
        &mut self,
        command_buffers: &[<Self as QueueRHI>::CommandBuffer],
    ) -> Result<(), Error> {
        unsafe {
            if command_buffers.is_empty() {
                return Ok(());
            }

            let device = self.device.upgrade().unwrap();
            let command_pool_idx = command_buffers
                .iter()
                .map(|cb| cb.command_pool_idx)
                .reduce(|acc, value| {
                    assert_eq!(
                        acc, value,
                        "Attempted to submit command buffers from different command pools",
                    );
                    value
                })
                .unwrap();
            for cb in command_buffers {
                // Good opportunity to handle device loss
                device.inner.end_command_buffer(cb.inner).unwrap();
            }

            let submit_info: Vec<_> = command_buffers
                .iter()
                .map(|cb| vk::CommandBufferSubmitInfo::default().command_buffer(cb.inner))
                .collect();

            let wait_semaphores: Vec<_> = command_buffers
                .iter()
                .flat_map(|cb| cb.wait.as_slice())
                .map(|info| {
                    vk::SemaphoreSubmitInfo::default()
                        .semaphore(info.semaphore)
                        .stage_mask(info.stage.into())
                        .value(info.value)
                })
                .collect();

            let signal_semaphores: Vec<_> = command_buffers
                .iter()
                .flat_map(|cb| cb.signal.as_slice())
                .map(|info| {
                    vk::SemaphoreSubmitInfo::default()
                        .semaphore(info.semaphore)
                        .stage_mask(info.stage.into())
                        .value(info.value)
                })
                .collect();
            let submits = [vk::SubmitInfo2::default()
                .command_buffer_infos(&submit_info)
                .signal_semaphore_infos(&signal_semaphores)
                .wait_semaphore_infos(&wait_semaphores)];
            // Good opportunity to handle device loss
            let queue = &mut self.queue; // .upgrade().unwrap();
            device
                .inner
                .queue_submit2(queue.queue, &submits, vk::Fence::null())
                .unwrap();

            // Store -1 to indicate command pool requires resetting
            queue.command_pools[command_pool_idx as usize]
                .used
                .store(-1, Ordering::Release);
        }
        Ok(())
    }
}

impl Drop for Queue {
    fn drop(&mut self) {
        let device = self.device.upgrade().unwrap();

        unsafe {
            device.inner.queue_wait_idle(self.queue.queue).unwrap();

            for command_pool in &self.queue.command_pools {
                command_pool.destroy(&device.inner)
            }
        }
    }
}

impl QueueRHI for Queue {
    type CommandBuffer = CommandBuffer;

    fn begin_recording(&mut self, command_pool: u32) -> Self::CommandBuffer {
        let command_buffer = self.get_command_buffer(command_pool);

        let begin_info = &vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        unsafe {
            // Good opportunity to handle device loss
            let device = self.device.upgrade().unwrap();
            device
                .inner
                .begin_command_buffer(command_buffer, begin_info)
                .unwrap();

            Self::CommandBuffer {
                device,
                heap: self.heap.upgrade().unwrap(),
                inner: command_buffer,
                command_pool_idx: command_pool,
                signal: vec![],
                wait: vec![],
                layout_transition_queue: vec![],
                presentation: None,
                render_pass_state: None,
            }
        }
    }

    unsafe fn submit(&mut self, command_buffers: &[Self::CommandBuffer]) -> Result<(), Error> {
        for cb in command_buffers {
            if let Some(swapchain_image) = cb.presentation {
                unsafe {
                    cb.multiple_layout_transition(&[LayoutTransition {
                        image: swapchain_image,
                        new_layout: vk::ImageLayout::PRESENT_SRC_KHR,
                        src_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                        src_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                        dst_stage_mask: vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
                        dst_access_mask: vk::AccessFlags2::empty(),
                    }]);
                }
            }
        }

        self.submit_impl(command_buffers)?;

        Ok(())
    }
}
