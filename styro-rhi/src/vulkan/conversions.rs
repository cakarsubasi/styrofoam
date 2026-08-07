use crate::*;
use ash::vk;

impl From<ash::vk::Result> for Error {
    fn from(value: ash::vk::Result) -> Self {
        match value {
            vk::Result::ERROR_DEVICE_LOST => Error::DeviceLost,
            vk::Result::ERROR_SURFACE_LOST_KHR => Error::SurfaceLost,
            vk::Result::ERROR_OUT_OF_DATE_KHR => Error::SwapchainOutOfDate,
            err => Error::OtherError(err.as_raw()),
        }
    }
}

impl From<CompareOp> for vk::CompareOp {
    fn from(value: CompareOp) -> Self {
        match value {
            CompareOp::Never => vk::CompareOp::NEVER,
            CompareOp::Less => vk::CompareOp::LESS,
            CompareOp::Equal => vk::CompareOp::EQUAL,
            CompareOp::LessOrEqual => vk::CompareOp::LESS_OR_EQUAL,
            CompareOp::Greater => vk::CompareOp::GREATER,
            CompareOp::NotEqual => vk::CompareOp::NOT_EQUAL,
            CompareOp::GreaterOrEqual => vk::CompareOp::GREATER_OR_EQUAL,
            CompareOp::Always => vk::CompareOp::ALWAYS,
        }
    }
}

impl From<StencilOp> for vk::StencilOp {
    fn from(value: StencilOp) -> Self {
        match value {
            StencilOp::Keep => vk::StencilOp::KEEP,
            StencilOp::Zero => vk::StencilOp::ZERO,
            StencilOp::Replace => vk::StencilOp::REPLACE,
            StencilOp::IncrementAndClamp => vk::StencilOp::INCREMENT_AND_CLAMP,
            StencilOp::DecrementAndClamp => vk::StencilOp::DECREMENT_AND_CLAMP,
            StencilOp::Invert => vk::StencilOp::INVERT,
            StencilOp::IncrementAndWrap => vk::StencilOp::INCREMENT_AND_WRAP,
            StencilOp::DecrementAndWrap => vk::StencilOp::DECREMENT_AND_WRAP,
        }
    }
}

impl From<BlendOp> for vk::BlendOp {
    fn from(value: BlendOp) -> Self {
        match value {
            BlendOp::Add => vk::BlendOp::ADD,
            BlendOp::Subtract => vk::BlendOp::SUBTRACT,
            BlendOp::ReverseSubtract => vk::BlendOp::REVERSE_SUBTRACT,
            BlendOp::Min => vk::BlendOp::MIN,
            BlendOp::Max => vk::BlendOp::MAX,
        }
    }
}

impl From<BlendFactor> for vk::BlendFactor {
    fn from(value: BlendFactor) -> Self {
        match value {
            BlendFactor::Zero => vk::BlendFactor::ZERO,
            BlendFactor::One => vk::BlendFactor::ONE,
            BlendFactor::SrcColor => vk::BlendFactor::SRC_COLOR,
            BlendFactor::OneMinusSrcColor => vk::BlendFactor::ONE_MINUS_SRC_COLOR,
            BlendFactor::DstColor => vk::BlendFactor::DST_COLOR,
            BlendFactor::OneMinusDstColor => vk::BlendFactor::ONE_MINUS_DST_COLOR,
            BlendFactor::SrcAlpha => vk::BlendFactor::SRC_ALPHA,
            BlendFactor::OneMinusSrcAlpha => vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
            BlendFactor::DstAlpha => vk::BlendFactor::DST_ALPHA,
            BlendFactor::OneMinusDstAlpha => vk::BlendFactor::ONE_MINUS_DST_ALPHA,
            BlendFactor::ConstantColor => vk::BlendFactor::CONSTANT_COLOR,
            BlendFactor::OneMinusConstantColor => vk::BlendFactor::ONE_MINUS_CONSTANT_COLOR,
            BlendFactor::ConstantAlpha => vk::BlendFactor::CONSTANT_ALPHA,
            BlendFactor::OneMinusConstantAlpha => vk::BlendFactor::ONE_MINUS_CONSTANT_ALPHA,
            BlendFactor::SrcAlphaSaturate => vk::BlendFactor::SRC_ALPHA_SATURATE,
            BlendFactor::Src1Color => vk::BlendFactor::SRC1_COLOR,
            BlendFactor::OneMinusSrc1Color => vk::BlendFactor::ONE_MINUS_SRC1_COLOR,
            BlendFactor::Src1Alpha => vk::BlendFactor::SRC1_ALPHA,
            BlendFactor::OneMinusSrc1Alpha => vk::BlendFactor::ONE_MINUS_SRC1_ALPHA,
        }
    }
}

impl From<LoadOp> for vk::AttachmentLoadOp {
    fn from(value: LoadOp) -> Self {
        match value {
            LoadOp::Load => vk::AttachmentLoadOp::LOAD,
            LoadOp::Clear => vk::AttachmentLoadOp::CLEAR,
            LoadOp::DontCare => vk::AttachmentLoadOp::DONT_CARE,
        }
    }
}

impl From<StoreOp> for vk::AttachmentStoreOp {
    fn from(value: StoreOp) -> Self {
        match value {
            StoreOp::Store => vk::AttachmentStoreOp::STORE,
            StoreOp::DontCare => vk::AttachmentStoreOp::DONT_CARE,
        }
    }
}

impl From<Clear> for vk::ClearValue {
    fn from(value: Clear) -> Self {
        // TODO: could use formats so we have one type of clear
        match value {
            Clear::Color(x, y, z, w) => vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [x, y, z, w],
                },
            },
            Clear::DepthStencil(depth, stencil) => vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: depth,
                    stencil: stencil,
                },
            },
        }
    }
}

impl From<Topology> for vk::PrimitiveTopology {
    fn from(value: Topology) -> Self {
        match value {
            Topology::PointList => vk::PrimitiveTopology::POINT_LIST,
            Topology::LineList => vk::PrimitiveTopology::LINE_LIST,
            Topology::LineStrip => vk::PrimitiveTopology::LINE_STRIP,
            Topology::TriangleList => vk::PrimitiveTopology::TRIANGLE_LIST,
            Topology::TriangleStrip => vk::PrimitiveTopology::TRIANGLE_STRIP,
            Topology::TriangleFan => vk::PrimitiveTopology::TRIANGLE_FAN,
        }
    }
}

impl Cull {
    pub(super) fn to_vk(&self) -> (vk::CullModeFlags, vk::FrontFace) {
        match self {
            Cull::CCW => (vk::CullModeFlags::BACK, vk::FrontFace::COUNTER_CLOCKWISE),
            Cull::CW => (vk::CullModeFlags::BACK, vk::FrontFace::CLOCKWISE),
            Cull::BOTH => (vk::CullModeFlags::FRONT_AND_BACK, vk::FrontFace::CLOCKWISE),
            Cull::NONE => (vk::CullModeFlags::NONE, vk::FrontFace::CLOCKWISE),
        }
    }
}

impl From<ImageType> for vk::ImageType {
    fn from(value: ImageType) -> Self {
        match value {
            ImageType::Type1D => vk::ImageType::TYPE_1D,
            ImageType::Type2D => vk::ImageType::TYPE_2D,
            ImageType::Type3D => vk::ImageType::TYPE_3D,
        }
    }
}
impl From<ImageType> for vk::ImageViewType {
    fn from(value: ImageType) -> Self {
        match value {
            ImageType::Type1D => vk::ImageViewType::TYPE_1D,
            ImageType::Type2D => vk::ImageViewType::TYPE_2D,
            ImageType::Type3D => vk::ImageViewType::TYPE_3D,
        }
    }
}
impl From<Filter> for vk::Filter {
    fn from(value: Filter) -> Self {
        match value {
            Filter::Nearest => vk::Filter::NEAREST,
            Filter::Linear => vk::Filter::LINEAR,
        }
    }
}

impl From<MipmapMode> for vk::SamplerMipmapMode {
    fn from(value: MipmapMode) -> Self {
        match value {
            MipmapMode::Nearest => vk::SamplerMipmapMode::NEAREST,
            MipmapMode::Linear => vk::SamplerMipmapMode::LINEAR,
        }
    }
}

impl From<SamplerAddressMode> for vk::SamplerAddressMode {
    fn from(value: SamplerAddressMode) -> Self {
        match value {
            SamplerAddressMode::Repeat => vk::SamplerAddressMode::REPEAT,
            SamplerAddressMode::MirroredRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
            SamplerAddressMode::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
            SamplerAddressMode::ClampToBorder => vk::SamplerAddressMode::CLAMP_TO_BORDER,
            SamplerAddressMode::MirrorClampToEdge => vk::SamplerAddressMode::MIRROR_CLAMP_TO_EDGE,
        }
    }
}

impl From<Stage> for vk::PipelineStageFlags2 {
    fn from(value: Stage) -> Self {
        match value {
            Stage::None => vk::PipelineStageFlags2::NONE,
            Stage::TopOfPipe => vk::PipelineStageFlags2::TOP_OF_PIPE,
            Stage::DrawIndirect => vk::PipelineStageFlags2::DRAW_INDIRECT,
            Stage::VertexShader => vk::PipelineStageFlags2::VERTEX_SHADER,
            Stage::FragmentShader => vk::PipelineStageFlags2::FRAGMENT_SHADER,
            Stage::EarlyFragmentTests => vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS,
            Stage::LateFragmentTests => vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
            Stage::ColorAttachmentOutput => vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            Stage::ComputeShader => vk::PipelineStageFlags2::COMPUTE_SHADER,
            Stage::Transfer => vk::PipelineStageFlags2::TRANSFER,
            Stage::BottomOfPipe => vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
            Stage::AllGraphics => vk::PipelineStageFlags2::ALL_GRAPHICS,
            Stage::AllCommands => vk::PipelineStageFlags2::ALL_COMMANDS,
            Stage::Host => vk::PipelineStageFlags2::HOST,
            Stage::Copy => vk::PipelineStageFlags2::COPY,
            Stage::Resolve => vk::PipelineStageFlags2::RESOLVE,
            Stage::Blit => vk::PipelineStageFlags2::BLIT,
            Stage::Clear => vk::PipelineStageFlags2::CLEAR,
            Stage::TaskShader => vk::PipelineStageFlags2::TASK_SHADER_EXT,
            Stage::MeshShader => vk::PipelineStageFlags2::MESH_SHADER_EXT,
        }
    }
}

impl From<ImageLayout> for vk::ImageLayout {
    fn from(value: ImageLayout) -> Self {
        match value {
            ImageLayout::Undefined => vk::ImageLayout::UNDEFINED,
            ImageLayout::General => vk::ImageLayout::GENERAL,
            ImageLayout::Attachment => vk::ImageLayout::ATTACHMENT_OPTIMAL,
            ImageLayout::ShaderReadOnlyOptimal => vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            ImageLayout::TransferSrcOptimal => vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            ImageLayout::TransferDstOptimal => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            ImageLayout::RenderingLocalRead => vk::ImageLayout::RENDERING_LOCAL_READ,
            ImageLayout::PresentSrc => vk::ImageLayout::PRESENT_SRC_KHR,
        }
    }
}
