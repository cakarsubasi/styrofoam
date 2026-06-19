#![allow(unsafe_op_in_unsafe_fn)]
pub mod commands;
#[macro_use]
mod debug;
pub mod shader;
pub mod vkhandles;

use ash;
use ash::vk;
use std::path::Path;
use std::sync::Arc;
use winit::raw_window_handle::RawDisplayHandle;
use winit::raw_window_handle::RawWindowHandle;

use crate::renderer::commands::*;
use crate::renderer::shader::Slangc;
use crate::renderer::vkhandles::Device;
use crate::renderer::vkhandles::Pipeline;
use crate::renderer::vkhandles::PresentationEngine;
use crate::renderer::vkhandles::Swapchain;
use crate::renderer::vkhandles::TargetFormat;

pub struct Renderer {
    _device: Arc<vkhandles::Device>,
    presentation_engine: PresentationEngine,
    state: RenderState,
}

struct RenderState {
    frame_index: u64,
    pipeline: Pipeline,
}

impl RenderState {
    fn new(device: &Arc<Device>, swapchain: &Swapchain) -> Self {
        let compiler = Slangc::new();
        let shader = compiler
            .compile(Path::new("res/shaders/triangle.slang"))
            .unwrap();
        let shader_module = Arc::clone(device).create_shader_module(&shader);

        let color_format = [swapchain.swapchain_format.format];
        let target_format = TargetFormat {
            color: &color_format,
            depth: None,
            stencil: None,
        };

        let pipeline = Arc::clone(device)
            .create_pipeline(&target_format, &shader_module)
            .unwrap();

        Self {
            frame_index: 0,
            pipeline,
        }
    }
}

#[derive(Debug)]
pub enum RendererError {
    DeviceLost,
    SurfaceLost,
    SwapchainOutOfDate,
    OtherError(vk::Result),
}

impl From<vk::Result> for RendererError {
    fn from(value: vk::Result) -> Self {
        match value {
            vk::Result::ERROR_DEVICE_LOST => RendererError::DeviceLost,
            vk::Result::ERROR_SURFACE_LOST_KHR => RendererError::SurfaceLost,
            vk::Result::ERROR_OUT_OF_DATE_KHR => RendererError::SwapchainOutOfDate,
            err => RendererError::OtherError(err),
        }
    }
}

impl Renderer {
    pub unsafe fn new(
        raw_display_handle: RawDisplayHandle,
        raw_window_handle: RawWindowHandle,
    ) -> Self {
        let instance = Arc::new(vkhandles::Instance::new(raw_display_handle));
        let surface = Arc::clone(&instance).create_surface(raw_display_handle, raw_window_handle);
        let device = Arc::new(instance.create_device(&surface));
        let swapchain = vkhandles::Swapchain::new(Arc::clone(&device), Arc::new(surface))
            .expect("Initial swapchain creation failure");
        let command_pool = Arc::clone(&device).create_command_pool().unwrap();
        let state = RenderState::new(&device, &swapchain);

        let presentation_engine = PresentationEngine::new(swapchain, command_pool);
        Self {
            _device: device,
            presentation_engine,
            state,
        }
    }

    pub fn request_redraw(&mut self) -> Result<(), RendererError> {
        // record or reuse command buffer
        let result = self
            .presentation_engine
            .next_frame(self.state.frame_index)
            .and_then(|presentation_context| {
                presentation_context.submit_and_present(|command_buffer, render_target| {
                    let color_attachments = [vk::RenderingAttachmentInfo::default()
                        .image_view(render_target.color_image_view)
                        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                        .load_op(vk::AttachmentLoadOp::CLEAR)
                        .store_op(vk::AttachmentStoreOp::STORE)
                        .clear_value(vk::ClearValue {
                            color: vk::ClearColorValue {
                                float32: [0.0, 0.0, 0.0, 1.0],
                            },
                        })];

                    let depth_attachment = vk::RenderingAttachmentInfo::default();
                    let stencil_attachment = vk::RenderingAttachmentInfo::default();

                    let rendering_info = vk::RenderingInfo::default()
                        .layer_count(1)
                        .view_mask(0)
                        .color_attachments(&color_attachments)
                        .depth_attachment(&depth_attachment)
                        .stencil_attachment(&stencil_attachment)
                        .render_area(vk::Rect2D {
                            offset: vk::Offset2D::default(),
                            extent: render_target.extent,
                        });

                    command_buffer.reset()?;

                    command_buffer.record(|command_buffer| {
                        command_buffer.transition_image_layout(
                            render_target.color_image,
                            vk::ImageLayout::UNDEFINED,
                            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                            vk::AccessFlags2::empty(),
                            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                        );

                        command_buffer.render(&rendering_info, |command_buffer| {
                            command_buffer.cmd_set_viewport(&[vk::Viewport {
                                x: 0.0,
                                y: 0.0,
                                width: render_target.extent.width as f32,
                                height: render_target.extent.height as f32,
                                min_depth: 0.0,
                                max_depth: 1.0,
                            }]);

                            command_buffer.cmd_set_scissor(&[vk::Rect2D {
                                offset: vk::Offset2D::default(),
                                extent: render_target.extent,
                            }]);

                            command_buffer.cmd_bind_pipeline(
                                vk::PipelineBindPoint::GRAPHICS,
                                &self.state.pipeline,
                            );
                            // Enter commands here
                            command_buffer.cmd_draw(3, 1, 0, 0);
                        });

                        command_buffer.transition_image_layout(
                            render_target.color_image,
                            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                            vk::ImageLayout::PRESENT_SRC_KHR,
                            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                            vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
                            vk::AccessFlags2::empty(),
                        );
                    })
                })
            });
        self.state.frame_index += 1;
        match result {
            Ok(()) => Ok(()),
            Err(vk::Result::SUBOPTIMAL_KHR) | Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                if self.presentation_engine.recreate_swapchain().is_err() {
                    Err(RendererError::SwapchainOutOfDate)
                } else {
                    Ok(())
                }
            }
            Err(err) => Err(err.into()),
        }
    }
}
