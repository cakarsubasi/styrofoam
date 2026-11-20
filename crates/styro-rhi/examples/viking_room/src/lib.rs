use std::ptr;
use std::{collections::HashMap, io::Cursor};
use styro_rhi::traits::*;
use styro_rhi::{self as rhi, SwapchainDeviceRHI as _};

use crate::TextureHeap::Heaps;

pub fn choose_surface_format(
    device: &rhi::Device,
    window_data: &rhi::WindowSystemData,
) -> rhi::SurfaceFormat {
    let surface_formats = device.enumerate_surface_formats(&window_data);
    let sdr_formats = || {
        surface_formats
            .iter()
            .filter(|format| matches!(format.color_space, rhi::ColorSpace::SrgbNonlinear))
    };
    let surface_format = sdr_formats()
        .find(|format| {
            matches!(
                format.format,
                rhi::Format::B8G8R8A8_Srgb | rhi::Format::R8G8B8A8_Srgb
            )
        })
        .or_else(|| sdr_formats().next())
        .expect("No SDR capable surface.");
    *surface_format
}

pub struct Mesh {
    pub positions: Vec<f32>,
    pub indices: Vec<u32>,
    pub vertex_data: Vec<VertexData>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct VertexData {
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

impl Mesh {
    /// Parse the obj into de-duplicated vertices. obj indexes positions, normals and texture coordinates
    /// separately, so every distinct `(position, texcoord, normal)` triple becomes one vertex.
    pub fn from_obj_bytes(bytes: &[u8]) -> Mesh {
        let model = obj::ObjData::load_buf(Cursor::new(bytes)).expect("Unable to parse obj");

        let mut mesh = Mesh {
            positions: Vec::new(),
            indices: Vec::new(),
            vertex_data: Vec::new(),
        };
        let mut seen: HashMap<(usize, Option<usize>, Option<usize>), u32> = HashMap::new();

        for polygon in model
            .objects
            .iter()
            .flat_map(|object| &object.groups)
            .flat_map(|group| &group.polys)
        {
            let corners = &polygon.0;
            // Triangulate as a fan, which is a no-op for triangles
            for i in 1..corners.len().saturating_sub(1) {
                for corner in [&corners[0], &corners[i], &corners[i + 1]] {
                    let key = (corner.0, corner.1, corner.2);
                    let index = *seen.entry(key).or_insert_with(|| {
                        let index = mesh.vertex_data.len() as u32;
                        mesh.positions.extend_from_slice(&model.position[corner.0]);
                        // obj texture coordinates have their origin at the bottom left, Vulkan at the top left
                        let uv = corner.1.map_or([0.0; 2], |t| {
                            [model.texture[t][0], 1.0 - model.texture[t][1]]
                        });
                        mesh.vertex_data.push(VertexData {
                            normal: corner.2.map_or([0.0; 3], |n| model.normal[n]),
                            uv,
                        });
                        index
                    });
                    mesh.indices.push(index);
                }
            }
        }
        mesh
    }
}

pub fn create_buffer_with_data(
    device: &mut rhi::Device,
    usage: rhi::BufferUsage,
    data: &[u8],
) -> rhi::GpuPtr {
    let buffer = device.create_buffer(&rhi::BufferInfo {
        memory: rhi::Memory::Upload,
        size: data.len() as u64,
        usage,
    });
    unsafe {
        ptr::copy_nonoverlapping(data.as_ptr(), device.buffer_host_addr(buffer), data.len());
    }
    buffer
}

pub fn create_texture_blocking(
    device: &mut rhi::Device,
    queue: &mut rhi::Queue,
    png_bytes: &[u8],
    format: rhi::Format,
) -> rhi::GpuImage {
    let pixels = image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png)
        .expect("Unable to parse image")
        .to_rgba8();
    let (width, height) = pixels.dimensions();

    let texture = device.create_image(&rhi::ImageInfo {
        dimensions: [width, height, 1],
        format,
        usage: rhi::ImageUsage::Sampled,
        ..Default::default()
    });
    let staging = create_buffer_with_data(device, rhi::BufferUsage::General, pixels.as_raw());

    // Can also transition to general instead one time, which should be optimal on most modern platforms
    let upload_semaphore = device.create_semaphore(0);
    let mut command_buffer = queue.begin_recording(0);
    command_buffer.image_barrier(
        rhi::Stage::TopOfPipe,
        rhi::Stage::Transfer,
        texture,
        rhi::ImageLayout::TransferDstOptimal,
    );
    command_buffer.copy_to_image(staging, texture.full_region());
    command_buffer.image_barrier(
        rhi::Stage::Transfer,
        rhi::Stage::FragmentShader,
        texture,
        rhi::ImageLayout::ShaderReadOnlyOptimal,
    );
    command_buffer.signal_after(rhi::Stage::BottomOfPipe, &upload_semaphore, 1);
    unsafe { queue.submit(&[command_buffer]).unwrap() };

    // Wait for the submission, after which the staging buffer is no longer in use
    upload_semaphore.wait(1);
    unsafe { device.destroy_buffer(staging) };

    texture
}

/// How the texture is made available to shaders
pub enum TextureHeap {
    /// Classic bindless. Textures are accessed through a single descriptor set.
    Classic,
    /// Descriptor heaps. Textures are accessed through a user written descriptor heap.
    Heaps {
        resource_heap: rhi::GpuPtr,
        sampler_heap: rhi::GpuPtr,
    },
}

const HEAP_DESCRIPTORS: usize = 64;

impl TextureHeap {
    pub fn new_classic(_device: &mut rhi::Device) -> Self {
        return Self::Classic;
    }

    pub fn new_descriptor_heap(device: &mut rhi::Device) -> Self {
        let props = device.get_descriptor_heap_properties();
        let descriptors_total_size = (HEAP_DESCRIPTORS * rhi::util::DESCRIPTOR_STRIDE) as u64;
        let res_alignment = props.resource_heap_alignment;
        let sampler_alignment = props.sampler_heap_alignment;
        let resource_size = (props.min_resource_heap_reserved_range + descriptors_total_size)
            .div_ceil(res_alignment)
            * res_alignment
            + res_alignment;
        let sampler_size = (props.min_sampler_heap_reserved_range + descriptors_total_size)
            .div_ceil(sampler_alignment)
            * sampler_alignment
            + sampler_alignment;

        let resource_heap = device.create_buffer(&rhi::BufferInfo {
            memory: rhi::Memory::Upload,
            size: resource_size,
            usage: rhi::BufferUsage::DescriptorHeap,
        });
        let sampler_heap = device.create_buffer(&rhi::BufferInfo {
            memory: rhi::Memory::Upload,
            size: sampler_size,
            usage: rhi::BufferUsage::DescriptorHeap,
        });

        Self::Heaps {
            resource_heap,
            sampler_heap,
        }
    }

    pub fn write_sampled_image_descriptor(
        &mut self,
        device: &mut rhi::Device,
        index: usize,
        texture: rhi::GpuImage,
        sampler: &rhi::SamplerInfo,
    ) {
        match self {
            TextureHeap::Classic => {
                device.write_sampled_image_descriptor(index, texture, sampler);
            }
            TextureHeap::Heaps {
                resource_heap,
                sampler_heap,
            } => {
                // Both of these update the pointers so that they are aligned and can be bound
                let mut resource_view = rhi::util::align_resource_heap(device, resource_heap);
                let mut sampler_view = rhi::util::align_sampler_heap(device, sampler_heap);

                let image_descriptor = device.get_image_descriptor(texture);
                let sampler_descriptor = device.get_sampler_descriptor(sampler);
                unsafe {
                    resource_view.as_mut_sliced(0, 1)[index] = image_descriptor;
                    sampler_view.as_mut_sliced(0, 1)[index] = sampler_descriptor;
                }
            }
        }
    }

    pub fn bind_heap(&mut self, command_buffer: &mut rhi::CommandBuffer) {
        // Only user managed descriptor heaps need to be bound
        if let TextureHeap::Heaps {
            resource_heap,
            sampler_heap,
        } = self
        {
            command_buffer.bind_descriptor_heap(*resource_heap, *sampler_heap);
        }
    }

    pub unsafe fn destroy(&mut self, device: &mut rhi::Device) {
        if let Heaps {
            resource_heap,
            sampler_heap,
        } = self
        {
            unsafe {
                device.destroy_buffer(*resource_heap);
                device.destroy_buffer(*sampler_heap);
            }
        }
    }
}
