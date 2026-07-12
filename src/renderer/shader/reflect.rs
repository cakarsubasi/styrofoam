#![allow(non_snake_case)]
use ash::vk;
use serde::Deserialize;

use crate::util::str::{SStr, c_string};
// Likely will need a lot of edits

pub mod EntryPoint {
    use super::*;

    /// Input argument
    #[derive(Debug, Deserialize)]
    pub struct Parameter {
        /// name of the argument
        pub name: String,
        /// semantic name of the argument, e.g. SV_VERTEXID
        pub semanticName: Option<String>,
        /// Type of the parameter
        pub r#type: ParameterType,

        /// stage of the parameter
        pub stage: Option<String>,
        /// binding information of the parameter
        pub binding: Option<Binding>,
    }

    /// Return value
    #[derive(Debug, Deserialize)]
    pub struct OutputParameter {
        pub r#type: ParameterType,

        pub stage: Option<String>,
        pub binding: Option<Binding>,
    }

    #[derive(Debug, Deserialize)]
    pub struct Binding {
        // The kind of the binding, for example varyingInput or varyingOutput
        pub kind: String,
        pub index: Option<u32>,
        pub count: Option<u32>,
    }

    #[derive(Debug, Deserialize)]
    pub struct ParameterType {
        /// scalar, struct, vector etc.
        pub kind: String,
        /// If kind is scalar, contains the name of the type
        /// e.g. uint32
        pub scalarType: Option<String>,
        /// If kind is struct, contains the name of the struct
        pub name: Option<String>,
        /// If kind is struct, contains its fields
        pub fields: Option<Vec<Parameter>>,
        /// If kind is vector, contains the number of elements
        pub elementCount: Option<u64>,
        /// If kind is vector, contains the type of its elements
        pub elementType: Option<Box<ParameterType>>,
    }
}

use EntryPoint::*;

#[derive(Debug, Deserialize)]
pub struct SREntryPoint {
    /// vertMain, fragMain etc.
    pub name: String,
    /// vertex, fragment, geometry etc.
    pub stage: String,
    // Input parameters
    pub parameters: Vec<Parameter>,
    // Output parameter
    pub result: OutputParameter,
    /// Bindings used by the entry point
    pub bindings: Vec<serde_json::Value>,
}

impl SREntryPoint {
    fn shader_stage(&self) -> ShaderStage {
        match self.stage.as_str() {
            "vertex" => ShaderStage::Vertex,
            "fragment" => ShaderStage::Fragment,
            _ => unreachable!(),
        }
    }
}

/// Shader reflection
#[derive(Debug, Deserialize)]
pub struct SR {
    pub parameters: Vec<serde_json::Value>,
    pub entryPoints: Vec<SREntryPoint>,
    pub bindlessSpaceIndex: u64,
}

#[derive(Debug, Clone, Copy)]
pub enum ShaderStage {
    Vertex,
    Fragment,
    // Compute,
    // Mesh,
}

impl From<ShaderStage> for vk::ShaderStageFlags {
    fn from(shader_stage: ShaderStage) -> Self {
        match shader_stage {
            ShaderStage::Vertex => vk::ShaderStageFlags::VERTEX,
            ShaderStage::Fragment => vk::ShaderStageFlags::FRAGMENT,
        }
    }
}

#[derive(Debug)]
pub struct ShaderInfo {
    pub entry_point: SStr,
    pub stage: ShaderStage,
    // parameters will most likely be added in the future
    _hidden: (),
}

// Reflection might expose multiple shaders
impl From<&SR> for Option<Vec<ShaderInfo>> {
    fn from(reflection: &SR) -> Self {
        Some(
            reflection
                .entryPoints
                .iter()
                .map(|entry_point| ShaderInfo {
                    entry_point: c_string(entry_point.name.clone()),
                    stage: entry_point.shader_stage(),
                    _hidden: (),
                })
                .collect::<Vec<_>>(),
        )
    }
}
