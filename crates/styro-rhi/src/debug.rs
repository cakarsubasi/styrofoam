use std::ffi::CStr;

use crate::{CommandRHI, QueueRHI};

pub trait DebugQueueRHI: QueueRHI {
    fn debug_label(&self, label: &CStr, color: [f32; 4]);

    fn debug_begin_region(&self, label: &CStr, color: [f32; 4]);

    fn debug_end_region(&self);
}

pub trait DebugCommandRHI: CommandRHI {
    fn debug_label(&self, label: &CStr, color: [f32; 4]);

    fn debug_begin_region(&self, label: &CStr, color: [f32; 4]);

    fn debug_end_region(&self);
}
