pub struct ResourceHeapPtr {
    cpu: *mut ImageDescriptor,
    len: usize,
    gpu: u64,
}

impl ResourceHeapPtr {
    pub fn host_address(&self) -> *mut ImageDescriptor {
        self.cpu
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn device_address(&self) -> u64 {
        self.gpu
    }

    pub unsafe fn as_mut(&mut self) -> &mut [ImageDescriptor] {
        unsafe { slice::from_raw_parts_mut(self.cpu, self.len) }
    }

    pub unsafe fn as_mut_sliced(&mut self, offset: usize, len: usize) -> &mut [ImageDescriptor] {
        todo!()
    }
}

pub fn align_resource_heap(device: &Device, heap_ptr: &mut GpuPtr) -> ResourceHeapPtr {
    let resource_heap_ptr = device.buffer_host_ptr(*heap_ptr);
    let offset = resource_heap_ptr.align_offset(32);
    heap_ptr.offset = offset as u32;
    heap_ptr.size -= offset as u32;

    let len = heap_ptr.size / 32;
    let host_addr = resource_heap_ptr as *mut ImageDescriptor;
    let device_addr = device.buffer_device_ptr(*heap_ptr);

    ResourceHeapPtr {
        cpu: host_addr,
        len: len as usize,
        gpu: device_addr,
    }
}
