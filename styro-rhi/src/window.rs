use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

use crate::{Error, Format, GpuPtr, Queue};

#[derive(Debug, Clone, Copy)]
pub enum WindowSystem {
    None = 0,
    AppKit = 1,
    UiKit = 2,
    Windows = 3,
    Xlib = 4,
    Xcb = 5,
    Wayland = 6,
    Headless = 7,
}

pub struct WindowSystemData {
    pub display_handle: RawDisplayHandle,
    pub window_handle: RawWindowHandle,
}

#[derive(Debug, Clone, Copy)]
pub enum ColorSpace {
    SrgbNonlinear,
}

#[derive(Clone, Copy)]
pub struct SwapchainInfo {
    pub size: u32,
    pub format: Format,
    pub color_space: ColorSpace,
}

pub trait SwapchainDeviceRHI {
    type Swapchain;

    fn create_swapchain(
        &self,
        queue: &Queue,
        window: &WindowSystemData,
        info: &SwapchainInfo,
    ) -> Self::Swapchain;
}

pub trait SwapchainCommandRHI {
    fn begin_presenting(&mut self, swapchain_image: GpuPtr);
}

pub trait SwapchainRHI {
    fn acquire_next_image(&mut self) -> Result<GpuPtr, Error>;

    fn present(&mut self, swapchain_image: GpuPtr) -> Result<(), Error>;
}
