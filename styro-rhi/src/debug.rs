use ash::{ext, vk};
use std::{borrow::Cow, ffi::CStr};

use super::*;

// TODO: might wish to redirect these errors to a UI component later
pub(super) unsafe extern "system" fn vulkan_debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user_data: *mut std::os::raw::c_void,
) -> vk::Bool32 {
    let callback_data = *p_callback_data;
    let message_id_number = callback_data.message_id_number;

    let message_id_name = if callback_data.p_message_id_name.is_null() {
        Cow::from("")
    } else {
        CStr::from_ptr(callback_data.p_message_id_name).to_string_lossy()
    };

    let message = if callback_data.p_message.is_null() {
        Cow::from("")
    } else {
        CStr::from_ptr(callback_data.p_message).to_string_lossy()
    };

    println!(
        "{message_severity:?}:{message_type:?} [{message_id_name} ({message_id_number})] : {message}\n",
    );
    vk::FALSE
}

/// # Safety: debug_utils_loader must be from a valid instance with
///           the VK_EXT_debug_utils extension enabled
pub(super) unsafe fn create_debug_messenger(
    debug_utils_loader: &ext::debug_utils::Instance,
) -> vk::DebugUtilsMessengerEXT {
    let debug_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
        .message_severity(
            vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
        )
        .message_type(
            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
        )
        .pfn_user_callback(Some(vulkan_debug_callback));

    let debug_messenger = debug_utils_loader
        .create_debug_utils_messenger(&debug_info, None)
        .unwrap();
    debug_messenger
}

// Convenient way to name objects for debug
pub trait DebugName {
    fn debug_name(self, name: &CStr) -> Self;
}

// Who needs proc macros anyway?
macro_rules! vk_debug_name_trait_impl {
    ($struct_type:ident) => {
        impl DebugName for $struct_type {
            fn debug_name(self, name: &std::ffi::CStr) -> Self {
                let debug_utils_object_name = vk::DebugUtilsObjectNameInfoEXT::default()
                    .object_handle(self.inner)
                    .object_name(name);
                unsafe {
                    self.device
                        .debug_utils_loader
                        .set_debug_utils_object_name(&debug_utils_object_name)
                        .unwrap();
                }
                self
            }
        }

        impl DebugName for &$struct_type {
            fn debug_name(self, name: &std::ffi::CStr) -> Self {
                let debug_utils_object_name = vk::DebugUtilsObjectNameInfoEXT::default()
                    .object_handle(self.inner)
                    .object_name(name);
                unsafe {
                    self.device
                        .debug_utils_loader
                        .set_debug_utils_object_name(&debug_utils_object_name)
                        .unwrap();
                }
                self
            }
        }
    };
}

//vk_debug_name_trait_impl! {Semaphore}
//vk_debug_name_trait_impl! {Fence}
//vk_debug_name_trait_impl! {CommandBuffer}
//vk_debug_name_trait_impl! {CommandPool}
//vk_debug_name_trait_impl! {Buffer}
//vk_debug_name_trait_impl! {Image}
//vk_debug_name_trait_impl! {Pipeline}
