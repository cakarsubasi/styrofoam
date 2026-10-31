use std::{collections::HashMap, fs::File};

use egui::epaint;
use styro_rendergraph::pass::GraphicsPipeline;
use styro_rendergraph::resource::{
    BufferDesc, BufferUsage, CombinedImageSampler, GpuPtr, ImageDesc, ImageHandle, ImageUsage,
    Persistent, Res,
};
use styro_rendergraph::{Attachment, Frame, Getter as _, Load};
use styro_rhi::{self as rhi};

pub struct UiData {
    pub triangles: Vec<egui::ClippedPrimitive>,
    pub textures: egui::TexturesDelta,
}

#[derive(bytemuck::Zeroable, bytemuck::Pod, Clone, Copy)]
#[repr(C)]
struct EguiVertex {
    pos: [f32; 2],  // 8
    uv: [f32; 2],   // 16
    color: [u8; 4], // 20
}

#[derive(bytemuck::Zeroable, bytemuck::Pod, Clone, Copy)]
#[repr(C)]
struct UiPushData {
    vertices: u64,
    texture: [i32; 2],
}

#[derive(bytemuck::NoUninit, Clone, Copy)]
#[repr(C)]
struct EguiPush {
    vertex: GpuPtr<epaint::Vertex>,
    image: CombinedImageSampler,
}

pub struct UiRenderer {
    shader: GraphicsPipeline<EguiPush>,
    textures: HashMap<u64, (Persistent<ImageHandle>, Res<ImageHandle>)>,
}

impl UiRenderer {
    pub fn new(device: &mut rhi::Device) -> Self {
        let mut spv = File::open("res/spirv/egui.spv").unwrap();
        let text = rhi::read_spv(&mut spv).unwrap();

        // TODO: simpler initialization
        let vertex_ir = rhi::ShaderIR {
            bytes: &text,
            entry: c"vertMain",
        };
        let frag_ir = rhi::ShaderIR {
            bytes: &text,
            entry: c"fragMain",
        };
        let description = rhi::RasterDescription {
            color_formats: &[rhi::Format::R8G8B8A8_Srgb],
            cull: rhi::Cull::NONE,
            ..Default::default()
        };

        Self {
            shader: GraphicsPipeline::new(device, &vertex_ir, &frag_ir, &description),
            textures: HashMap::new(),
        }
    }

    pub fn run<'frame>(
        &'frame mut self,
        frame: &mut Frame<'_, 'frame>,
        window_image: Res<ImageHandle>,
        update: UiData,
    ) {
        let UiData {
            triangles,
            textures,
        } = update;

        for id in textures.free {
            match id {
                egui::TextureId::User(id) => {
                    if let Some((_, res)) = self.textures.remove(&id) {
                        // Maybe I should do something here...
                        frame.delete(res);
                    }
                }
                _ => panic!(),
            }
        }

        for (id, update) in textures.set {
            // TODO: actually is two different sizes
            let size = update.image.size();
            let egui::TextureId::User(id) = id else {
                panic!()
            };
            if let Some(_) = update.pos {
                let (persist, _) = self.textures.get(&id).unwrap();
                let mut texture_update_pass = frame.new_pass("ui texture update");
                let texture = texture_update_pass.output_ref(
                    ImageDesc::sampled_2d(size[0], size[1], rhi::Format::R8G8B8A8_Srgb),
                    ImageUsage::transfer().persist(&persist),
                );

                texture_update_pass.build(move |mut ctx| {
                    let egui::ImageData::Color(data) = update.image;
                    ctx.copy_to_image(texture, data.pixels.as_slice() /* ... */);
                })
            } else {
                // Brand new texture
                let mut texture_upload_pass = frame.new_pass("ui texture upload");
                let persist = Persistent::new();
                let texture = texture_upload_pass.output_ref(
                    ImageDesc::sampled_2d(size[0], size[1], rhi::Format::R8G8B8A8_Srgb),
                    ImageUsage::transfer().persist(&persist),
                );

                self.textures.insert(id, (persist, texture));

                texture_upload_pass.build(move |mut ctx| {
                    let egui::ImageData::Color(data) = update.image;
                    ctx.copy_to_image(texture, data.pixels.as_slice() /* ... */)
                });
            }
        }

        let sampler = frame.sampler(&rhi::SamplerInfo::default());

        for prim in triangles {
            match prim.primitive {
                egui::epaint::Primitive::Mesh(mesh) => {
                    let egui::TextureId::User(id) = mesh.texture_id else {
                        panic!()
                    };
                    let &(_, texture) = self.textures.get(&id).unwrap();

                    let mut upload_pass = frame.new_pass("ui upload");

                    let vertex_buf = upload_pass.output_ref(
                        BufferDesc::upload(mesh.vertices.len() * size_of::<epaint::Vertex>()),
                        BufferUsage::write(),
                    );
                    let index_buf = upload_pass.output_ref(
                        BufferDesc::upload(mesh.indices.len() * size_of::<u32>()),
                        BufferUsage::write(),
                    );

                    upload_pass.build(move |mut ctx| {
                        ctx.copy_to_buffer(vertex_buf, mesh.vertices.as_slice());
                        ctx.copy_to_buffer(index_buf, mesh.indices.as_slice());
                    });

                    let mut draw_pass = frame.new_pass("ui draw");
                    draw_pass.input_ref(vertex_buf, BufferUsage::read());
                    draw_pass.input_ref(index_buf, BufferUsage::index());
                    draw_pass.input_ref(window_image, ImageUsage::color_write());
                    draw_pass.input_ref(texture, ImageUsage::sampled());

                    let shader = &self.shader;
                    draw_pass.build(move |mut ctx| {
                        let vertex_buf = ctx.get(vertex_buf);
                        let index_buf = ctx.get(index_buf);
                        let texture = ctx.get(texture);

                        let push = EguiPush {
                            vertex: vertex_buf.ptr(),
                            image: CombinedImageSampler::new(texture, sampler),
                        };

                        let mut render_pass = ctx.render_pass(
                            &[Attachment {
                                image: window_image,
                                load: Load::Load,
                                store: true,
                            }],
                            None,
                            None,
                        );
                        //render_pass.scissor(prim.clip_rect);
                        shader.draw_indexed(&mut render_pass, &push, index_buf, 0);
                    });
                }
                egui::epaint::Primitive::Callback(_callback) => panic!("unsupported"),
            }
        }
    }
}
