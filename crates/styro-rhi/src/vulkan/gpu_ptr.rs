/// Handle to a buffer resource, or a range within one
///
/// Create with [crate::DeviceRHI::create_buffer].
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct GpuPtr {
    pub(super) handle: u64,
    pub offset: u32,
    pub size: u32,
}

impl GpuPtr {
    pub fn null() -> Self {
        Self {
            handle: 0,
            offset: 0,
            size: 0,
        }
    }

    pub fn is_null(&self) -> bool {
        self.handle == 0
    }
}
