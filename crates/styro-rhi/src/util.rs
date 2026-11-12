//!
//! Descriptor heaps are plain buffers created with [crate::BufferUsage::DescriptorHeap]. These helpers
//! align such a buffer and give typed access to the descriptors inside it.
use std::slice;

use crate::Device;
use crate::{DeviceRHI as _, GpuPtr, ImageDescriptor, SamplerDescriptor};

/// Size in bytes of one descriptor slot in a heap. This should matches the `-spirv-resource-heap-stride`
/// and `-spirv-sampler-heap-stride` arguments passed to `slangc`.
///
/// Setting this to 32 excludes a bunch of slightly older AMD GPUs which is too bad.
pub const DESCRIPTOR_STRIDE: usize = 32;

/// A host mapped, typed view of an aligned descriptor heap
///
/// See [align_resource_heap] and [align_sampler_heap].
pub struct DescriptorHeapPtr<T> {
    cpu: *mut T,
    len: usize,
    gpu: u64,
}

/// Host view of a resource heap holding [ImageDescriptor]s
pub type ResourceHeapPtr = DescriptorHeapPtr<ImageDescriptor>;
/// Host view of a sampler heap holding [SamplerDescriptor]s
pub type SamplerHeapPtr = DescriptorHeapPtr<SamplerDescriptor>;

impl<T> DescriptorHeapPtr<T> {
    /// Host address of the first descriptor
    pub fn host_address(&self) -> *mut T {
        self.cpu
    }

    /// Number of usable descriptors, not counting the reserved range at the end of the heap
    pub fn len(&self) -> usize {
        self.len
    }

    /// Device address of the first descriptor
    pub fn device_address(&self) -> u64 {
        self.gpu
    }

    /// All descriptors in the heap
    ///
    /// # Safety
    /// - The heap buffer must be live and mapped.
    /// - The returned slice must not alias any other access to the heap, and the device must not be
    /// reading the written descriptors while they are modified.
    pub unsafe fn as_mut(&mut self) -> &mut [T] {
        unsafe { slice::from_raw_parts_mut(self.cpu, self.len) }
    }

    /// `len` descriptors starting at descriptor `offset`
    ///
    /// # Panics
    /// `offset + len` is greater than [DescriptorHeapPtr::len].
    ///
    /// # Safety
    /// - The heap buffer must be live and mapped.
    /// - The returned slice must not alias any other access to the heap, and the device must not be
    /// reading the written descriptors while they are modified.
    pub unsafe fn as_mut_sliced(&mut self, offset: usize, len: usize) -> &mut [T] {
        let end = offset.checked_add(len).expect("Descriptor range overflow");
        assert!(
            end <= self.len,
            "Descriptor range is out of bounds for the heap"
        );
        unsafe { slice::from_raw_parts_mut(self.cpu.add(offset), len) }
    }
}

fn align_heap<T>(
    device: &Device,
    heap_ptr: &mut GpuPtr,
    minimum_alignment: usize,
    reserved_bytes: usize,
) -> DescriptorHeapPtr<T> {
    assert!(!heap_ptr.is_null(), "Heap must not be the null pointer");
    let host_ptr = device.buffer_host_addr(*heap_ptr);
    let padding = host_ptr.align_offset(minimum_alignment);
    heap_ptr.offset += padding as u32;
    heap_ptr.size -= padding as u32;

    let usable = (heap_ptr.size as usize)
        .checked_sub(reserved_bytes)
        .expect("Heap is smaller than its reserved range");

    DescriptorHeapPtr {
        // Safety: padding is inside the buffer, since `heap_ptr.size` did not underflow
        cpu: unsafe { host_ptr.add(padding) } as *mut T,
        len: usable / DESCRIPTOR_STRIDE,
        gpu: device.buffer_device_addr(*heap_ptr),
    }
}

/// Align a resource heap buffer and get a host view of its [ImageDescriptor]s
///
/// # Valid usage
/// - `heap_ptr` must be a buffer created with [crate::BufferUsage::DescriptorHeap] and a host visible
/// [crate::Memory] type
/// - The descriptor heap feature must be enabled
pub fn align_resource_heap(device: &Device, heap_ptr: &mut GpuPtr) -> ResourceHeapPtr {
    let props = device.get_descriptor_heap_properties();
    // hopefully only meme platforms
    assert!(
        props.image_descriptor_size as usize <= DESCRIPTOR_STRIDE,
        "Image descriptors do not fit in the heap stride"
    );
    align_heap(
        device,
        heap_ptr,
        DESCRIPTOR_STRIDE.max(props.resource_heap_alignment as usize),
        props.min_resource_heap_reserved_range as usize,
    )
}

/// Align a sampler heap buffer and get a host view of its [SamplerDescriptor]s
///
/// # Valid usage
/// - `heap_ptr` must be a buffer created with [crate::BufferUsage::DescriptorHeap] and a host visible
/// [crate::Memory] type
/// - The descriptor heap feature must be enabled
pub fn align_sampler_heap(device: &Device, heap_ptr: &mut GpuPtr) -> SamplerHeapPtr {
    let props = device.get_descriptor_heap_properties();
    // hopefully only meme platforms
    assert!(
        props.sampler_descriptor_size as usize <= DESCRIPTOR_STRIDE,
        "Sampler descriptors do not fit in the heap stride"
    );
    align_heap(
        device,
        heap_ptr,
        DESCRIPTOR_STRIDE.max(props.sampler_heap_alignment as usize),
        props.min_sampler_heap_reserved_range as usize,
    )
}
