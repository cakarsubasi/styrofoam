use std::{
    alloc::{AllocError, Allocator},
    ffi::c_void,
    ptr::NonNull,
};

/// # Safety:
/// - `aligned_alloc` must be a correctly implemented `aligned_alloc` function
/// - `free` must be a correctly implemented `free` function belonging to the same allocator
/// - Users of AllocWrapper must ensure that it is live not any longer than its members
pub unsafe extern "C" fn wrap_aligned_alloc(
    aligned_alloc: unsafe extern "C" fn(usize, usize) -> *mut c_void,
    free: unsafe extern "C" fn(*mut c_void),
) -> AllocWrapper {
    AllocWrapper {
        aligned_alloc,
        free,
    }
}

#[repr(C)]
pub struct AllocWrapper {
    aligned_alloc: unsafe extern "C" fn(usize, usize) -> *mut c_void,
    free: unsafe extern "C" fn(*mut c_void),
}

unsafe impl Allocator for AllocWrapper {
    fn allocate(
        &self,
        layout: std::alloc::Layout,
    ) -> Result<std::ptr::NonNull<[u8]>, std::alloc::AllocError> {
        let size = layout.size();
        let align = layout.align();
        if size == 0 {
            return Ok(NonNull::slice_from_raw_parts(layout.dangling_ptr(), 0));
        }

        // Safety: creator of AllocWrapper promised to provide a correct and live aligned_alloc
        unsafe {
            let ptr = (self.aligned_alloc)(size, align) as *mut u8;
            let ptr = NonNull::new(ptr);
            if let Some(ptr) = ptr {
                Ok(NonNull::slice_from_raw_parts(ptr, size))
            } else {
                Err(AllocError)
            }
        }
    }

    unsafe fn deallocate(&self, ptr: std::ptr::NonNull<u8>, layout: std::alloc::Layout) {
        if layout.size() != 0 {
            // Safety: Creator of AllocWrapper promised to provide a correct and live free
            unsafe { (self.free)(ptr.as_ptr() as *mut c_void) }
        }
    }
}
