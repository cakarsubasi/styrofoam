use ash::vk;

use crate::*;

/// Handle to an image resource
///
/// Create with [DeviceRHI::create_image].
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct GpuImage {
    pub(super) handle: u64,
    pub(super) desc: ImageInfo,
}

impl GpuImage {
    /// Create an invalid image handle
    pub fn null() -> Self {
        Self {
            handle: 0,
            desc: ImageInfo::default(),
        }
    }

    pub fn is_null(&self) -> bool {
        self.handle == 0
    }

    /// The description the image was created with
    pub fn info(&self) -> &ImageInfo {
        &self.desc
    }

    /// Dimensions of the image
    pub fn dimensions(&self) -> UVec3 {
        self.desc.dimensions
    }

    /// Region covering the whole image
    pub fn full_region(self) -> GpuImageRegion {
        self.crop([0, 0, 0], self.desc.dimensions)
    }

    /// Region of the image starting at `offset` with size `extent`
    pub fn crop(self, offset: UVec3, extent: UVec3) -> GpuImageRegion {
        GpuImageRegion {
            image: self,
            offset,
            extent,
        }
    }
}

/// An image region
///
/// Used by transfer commands involving images.
///
/// # Valid usage
/// - `offset + extent` must not exceed the dimensions of `image` in any axis
#[derive(Debug, Clone, Copy)]
pub struct GpuImageRegion {
    pub image: GpuImage,
    pub offset: UVec3,
    pub extent: UVec3,
}

impl GpuImageRegion {
    /// Check that the region lies within the bounds of its image
    pub fn is_within_bounds(&self) -> bool {
        let dims = self.image.desc.dimensions;
        (0..3).all(|i| {
            self.offset[i]
                .checked_add(self.extent[i])
                .is_some_and(|end| end <= dims[i])
        })
    }

    pub(super) fn vk_offset(&self) -> vk::Offset3D {
        vk::Offset3D {
            x: self.offset[0] as i32,
            y: self.offset[1] as i32,
            z: self.offset[2] as i32,
        }
    }

    /// Exclusive upper corner of the region
    pub(super) fn vk_end_offset(&self) -> vk::Offset3D {
        vk::Offset3D {
            x: (self.offset[0] + self.extent[0]) as i32,
            y: (self.offset[1] + self.extent[1]) as i32,
            z: (self.offset[2] + self.extent[2]) as i32,
        }
    }

    pub(super) fn vk_extent(&self) -> vk::Extent3D {
        vk::Extent3D {
            width: self.extent[0],
            height: self.extent[1],
            depth: self.extent[2],
        }
    }

    pub(super) fn vk_subresource(&self) -> vk::ImageSubresourceLayers {
        vk::ImageSubresourceLayers::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .mip_level(0) // TODO: maybe consider multiple mip levels?
            .base_array_layer(0)
            .layer_count(self.image.desc.layer_count)
    }
}

impl From<GpuImage> for GpuImageRegion {
    fn from(image: GpuImage) -> Self {
        image.full_region()
    }
}
