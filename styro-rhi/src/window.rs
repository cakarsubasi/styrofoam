//! # Window System Integration
//!
//! Every platform has its own platform specific way of creating presentable surfaces
//! and interacting with them. This module provides both surfaces and swapchains under
//! a single [Swapchain] abstraction. In addition, swapchain management has been
//! simplified. Swapchain synchronization is performed automatically and the callers
//! only need to ensure ensure that they do not perform more concurrent presentation
//! operations than the swapchain size, which can trivially be be done as seen in the example.
//! Suboptimal surfaces are also handled automatically although it is recommended to pause
//! rendering during a resize to prevent exccessive recreation of swapchain images.
//!
//! # Example
//! ```
//! let swapchain = device.create_swapchain(
//!     &graphics_queue,
//!     &unsafe { WindowSystemData::from_raw_handles(raw_display_handle, raw_window_handle) },
//!     &SwapchainInfo {
//!         size: FRAMES_IN_FLIGHT as u32,
//!         format: Format::R8G8B8A8_Srgb,
//!         color_space: ColorSpace::SrgbNonlinear,
//!     },
//! );
//!
//! let frame_semaphore = device.create_semaphore(0);
//! let mut next_frame = 1;
//! loop {
//!     if next_frame > FRAMES_IN_FLIGHT {
//!         frame_semaphore.wait(next_frame - FRAMES_IN_FLIGHT);
//!     }
//!
//!     let swapchain_image = swapchain.acquire_next_image().unwrap();
//!
//!     let mut command_buffer = graphics_queue.begin_recording(next_frame % FRAMES_IN_FLIGHT);
//!     command_buffer.begin_presenting(swapchain_image);
//!
//!     // Perform rendering commands here
//!
//!     command_buffer.signal_after(Stage::BottomOfPipe, frame_semaphore, next_frame);
//!
//!     graphics_queue.submit(&[command_buffer]).unwrap();
//!     swapchain.present(swapchain_image).unwrap();
//!
//!     next_frame += 1;
//! }
//! ```
//!
//! # Notes
//! Although a surface and a swapchain are distinct entities, they are associated in
//! enough ways that we fully encapsulate the surface within the swapchain. Most
//! notably, a surface **must** be live for the lifetime of a swapchain and a surface
//! can only be associated with a single swapchain at any time.

use std::{ffi::c_void, num::NonZero, ptr::NonNull};

use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

use crate::{Error, Format, GpuPtr, Queue, QueueRHI};

/// The window system type
///
/// Since different window systems require different extensions, this is used to
/// ensure to load the correct extension during device initialization.
#[derive(Debug, Clone, Copy)]
pub enum WindowSystem {
    /// No window system. No extensions will be loaded. Most swapchain related
    /// functionality will panic.
    None = 0,
    /// AppKit window system. Found on Apple platforms.
    AppKit = 1,
    /// UiKit window system. Found on Apple platforms.
    UiKit = 2,
    /// Windows window system. Found on Microsoft Windows platforms.
    Windows = 3,
    /// Xlib window system. Found on Linux platforms.
    Xlib = 4,
    /// Xcb window system. Found on Linux platforms.
    Xcb = 5,
    /// Wayland window system. Found on Linux platforms.
    Wayland = 6,
    /// A headless surface. Not supported at the moment.
    Headless = 7,
}

/// Opaque type for window system data
#[derive(Debug, Clone, Copy)]
pub struct WindowSystemData {
    pub(crate) display_handle: RawDisplayHandle,
    pub(crate) window_handle: RawWindowHandle,
}

impl WindowSystemData {
    /// Create a WindowSystemData object from given raw handles
    ///
    /// # Safety
    /// `display_handle` and `window_handle` need to be valid handles and the OS specific
    /// objects associated with them need to be live for the duration of any Swapchain
    /// created from this WindowSystemData.
    ///
    /// Different platforms may also have their own additional rules on how these handles
    /// may be used.
    pub unsafe fn from_raw_handles(
        display_handle: RawDisplayHandle,
        window_handle: RawWindowHandle,
    ) -> Self {
        Self {
            display_handle,
            window_handle,
        }
    }

    /// Create a WindowSystemData object on an AppKit platform.
    pub unsafe fn from_app_kit(window_handle: NonNull<c_void>) -> Self {
        let display_handle =
            RawDisplayHandle::AppKit(raw_window_handle::AppKitDisplayHandle::new());
        let window_handle =
            RawWindowHandle::AppKit(raw_window_handle::AppKitWindowHandle::new(window_handle));

        unsafe { Self::from_raw_handles(display_handle, window_handle) }
    }

    /// Create a WindowSystemData object on a UiKit platform.
    pub unsafe fn from_ui_kit(window_handle: NonNull<c_void>) -> Self {
        let display_handle = RawDisplayHandle::UiKit(raw_window_handle::UiKitDisplayHandle::new());
        let window_handle =
            RawWindowHandle::UiKit(raw_window_handle::UiKitWindowHandle::new(window_handle));

        unsafe { Self::from_raw_handles(display_handle, window_handle) }
    }

    /// Create a WindowSystemData object on a Win32 platform.
    pub unsafe fn from_win32(window_handle: NonZero<isize>) -> Self {
        let display_handle = RawDisplayHandle::UiKit(raw_window_handle::UiKitDisplayHandle::new());
        let window_handle =
            RawWindowHandle::Win32(raw_window_handle::Win32WindowHandle::new(window_handle));

        unsafe { Self::from_raw_handles(display_handle, window_handle) }
    }

    /// Create a WindowSystemData object on a WinRT platform.
    pub unsafe fn from_winrt(window_handle: NonNull<c_void>) -> Self {
        let display_handle = RawDisplayHandle::UiKit(raw_window_handle::UiKitDisplayHandle::new());
        let window_handle =
            RawWindowHandle::WinRt(raw_window_handle::WinRtWindowHandle::new(window_handle));

        unsafe { Self::from_raw_handles(display_handle, window_handle) }
    }

    /// Create a WindowSystemData object on a Wayland platform.
    pub unsafe fn from_wayland(
        display_handle: NonNull<c_void>,
        window_handle: NonNull<c_void>,
    ) -> Self {
        let display_handle =
            RawDisplayHandle::Wayland(raw_window_handle::WaylandDisplayHandle::new(display_handle));
        let window_handle =
            RawWindowHandle::Wayland(raw_window_handle::WaylandWindowHandle::new(window_handle));

        unsafe { Self::from_raw_handles(display_handle, window_handle) }
    }

    /// Create a WindowSystemData object on an Xlib platform.
    pub unsafe fn from_xlib(
        display_handle: *mut c_void,
        screen_handle: i32,
        window_handle: u32,
    ) -> Self {
        let display_handle = NonNull::new(display_handle);

        let display_handle = RawDisplayHandle::Xlib(raw_window_handle::XlibDisplayHandle::new(
            display_handle,
            screen_handle,
        ));
        let window_handle =
            RawWindowHandle::Xlib(raw_window_handle::XlibWindowHandle::new(window_handle));

        unsafe { Self::from_raw_handles(display_handle, window_handle) }
    }

    /// Create a WindowSystemData object on an Xcb platform.
    pub unsafe fn from_xcb(
        display_handle: *mut c_void,
        screen_handle: i32,
        window_handle: NonZero<u32>,
    ) -> Self {
        let display_handle = NonNull::new(display_handle);

        let display_handle = RawDisplayHandle::Xcb(raw_window_handle::XcbDisplayHandle::new(
            display_handle,
            screen_handle,
        ));
        let window_handle =
            RawWindowHandle::Xcb(raw_window_handle::XcbWindowHandle::new(window_handle));

        unsafe { Self::from_raw_handles(display_handle, window_handle) }
    }

    #[allow(unused)]
    pub unsafe fn from_android(window_handle: NonNull<c_void>) -> Self {
        panic!(
            "One day, hardware vendors releasing on Android will manage to ship functional
            drivers which makes modern Vulkan on android usable. That day is not today."
        )
    }
}

/// Supported color space of the presentation engine
#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub enum ColorSpace {
    /// ITU-R Rec. 709 primaries with 2.2 Gamma. Supported everywhere.
    SrgbNonlinear,
}

/// Swapchain creation info.
///
/// # Valid usage
/// - size should be at least 1.
// TODO: should probably expose these somehow
/// - size should not be greater than the maximum swapchain size.
/// - format should be a supported swapchain format.
#[derive(Clone, Copy)]
pub struct SwapchainInfo {
    /// Maximum number of active images. Usually should be your maximum frames in flight.
    ///
    /// This is not the same thing as the backing images the swapchain has, as typically
    /// there should be more images available than there being actively presented to
    /// avoid stalling the rendering.
    pub size: u32,
    /// Swapchain format
    pub format: Format,
    /// Swapchain color space
    pub color_space: ColorSpace,
}

/// Device level functions for Swapchain functionality
pub trait SwapchainDeviceRHI {
    type Swapchain: SwapchainRHI;
    type Queue: QueueRHI;

    /// Create a swapchain and a surface
    ///
    /// # Valid usage
    /// - Device must be created with a [WindowSystem] that is not None.
    /// - Queue must support presentation. Graphics queues always support presentation.
    /// - [WindowSystemData] must be valid and correspond to the same window system
    /// which the device was created with.
    /// - [SwapchainInfo] must be valid.
    /// - Multiple swapchains must not be created with the same WindowSystemData.
    ///
    /// # Lifetime notes
    /// The swapchain and surface share a lifetime
    fn create_swapchain(
        &self,
        queue: &Queue,
        window: &WindowSystemData,
        info: &SwapchainInfo,
    ) -> Self::Swapchain;
}

/// Command buffer level functions for swapchain functionality
pub trait SwapchainCommandRHI {
    /// Prepare the swapchain image for presentation
    ///
    /// # Valid usage
    /// - `swapchain_image` must be previously acquired from a call to [SwapchainRHI::acquire_next_image]
    /// - Must only be called one-time during command buffer recording for a given `swapchain_image.
    fn begin_presenting(&mut self, swapchain_image: GpuPtr);
}

/// Swapchain functions
pub trait SwapchainRHI {
    /// Retrieve the opaque handle of the next available presentable image
    ///
    /// # Valid usage
    /// - This function must not be called more times than [SwapchainRHI::present] plus
    /// the size of the swapchain.
    /// - All swapchain images acquired from this function must eventually be presented.
    fn acquire_next_image(&mut self) -> Result<GpuPtr, Error>;

    /// Queue a presentation task for the previously acquired swapchain image
    ///
    /// # Valid usage
    /// - `swapchain_image` must be previously acquired from a call to [SwapchainRHI::acquire_next_image]
    /// - `swapchain_image` must not have a pending presentation operation
    fn present(&mut self, swapchain_image: GpuPtr) -> Result<(), Error>;
}
