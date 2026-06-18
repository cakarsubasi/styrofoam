pub mod reflect;

use std::{
    env,
    ffi::{OsStr, OsString},
    fs::File,
    hash::{DefaultHasher, Hash, Hasher},
    io::Read,
    path::Path,
    process::Command,
};

use crate::renderer::shader::reflect::SR;

pub struct SlangModule {
    pub(super) spirv: SpirvModule,
    pub(super) reflection: Option<SR>,
}

pub struct SpirvModule {
    pub(super) text: Vec<u8>,
}

#[derive(Debug)]
pub enum ShaderCreationError {
    IoError,
    SlangcError(Vec<String>),
}

pub struct Slangc {
    _unused: (),
}

impl Slangc {
    pub fn new() -> Self {
        let mut slangc = Command::new("slangc.exe");

        slangc.spawn().expect("Failed to find slangc");

        Slangc { _unused: () }
    }

    pub fn compile(&self, shader_path: &Path) -> Result<SlangModule, ShaderCreationError> {
        let mut hasher = DefaultHasher::new();
        shader_path.hash(&mut hasher);
        let hash = hasher.finish();

        // TODO: more robust error handling here
        let shader_name = shader_path.file_prefix().unwrap();
        let mut output_name = OsString::new();
        output_name.push(&shader_name);
        output_name.push(format!("{}.spv", hash));

        let mut reflection_name = OsString::new();
        reflection_name.push(&shader_name);
        reflection_name.push(format!("{}_reflection.json", hash));

        let temp_dir = env::temp_dir();

        let output_path = temp_dir.join(&output_name);
        let reflection_path = temp_dir.join(&reflection_name);

        let output = Command::new("slangc.exe")
            .arg(shader_path)
            .args(["-target", "spirv"])
            .args(["-profile", "spirv_1_4"])
            .arg("-fvk-use-entrypoint-name")
            //.args(["-entry", "vertMain"])
            //.args(["-entry", "fragMain"])
            .args([OsStr::new("-reflection-json"), reflection_path.as_os_str()])
            .args([OsStr::new("-o"), output_path.as_os_str()])
            .output()
            .expect("Failed to compile?");

        let errors = output.stderr;

        if !output.status.success() {
            eprintln!("{}", &String::from_utf8_lossy(&errors));
        }

        // json must be valid utf-8
        let reflection =
            std::fs::read_to_string(&reflection_path).expect("Failed to read reflection?");
        let mut shader_text = vec![];

        let mut shader_file = File::open(&output_path).unwrap();
        shader_file.read_to_end(&mut shader_text).unwrap();

        let reflection = serde_json::from_str(&reflection)
            .map_err(|err| println!("{}", err))
            .ok();

        Ok(SlangModule {
            spirv: SpirvModule { text: shader_text },
            reflection,
        })
    }
}

#[cfg(test)]
mod tests {

    use std::io::Write;

    use super::*;

    #[test]
    fn run_slangc() {
        let root = std::env::current_dir().unwrap();
        let shader_root = root.join("res").join("shaders");
        let shader = shader_root.join("triangle.slang");
        let shader = Slangc::new().compile(&shader).unwrap();

        let mut refl = File::create("refl.json").unwrap();

        write!(&mut refl, "{:?}", shader.reflection).unwrap();
        dbg!(&shader.reflection);
    }
}
