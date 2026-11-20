use std::{env, path::PathBuf, process::Command};

/// Compile `src/texture.slang` into `$OUT_DIR/<name>.spv`, with the given extra slangc arguments.
fn compile(shader: &str, name: &str, extra_args: &[&str]) {
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join(format!("{name}.spv"));

    let status = Command::new("slangc")
        .arg(shader)
        .args(["-target", "spirv"])
        .arg("-fvk-use-entrypoint-name")
        .args(extra_args)
        .arg("-o")
        .arg(&out)
        .status()
        .expect("failed to run `slangc`; install the Vulkan SDK and make sure it is on PATH");

    assert!(
        status.success(),
        "slangc failed to compile {shader} ({name})"
    );
}

fn main() {
    let shader = "src/mesh_3d.slang";
    println!("cargo:rerun-if-changed={shader}");
    println!("cargo:rerun-if-changed=build.rs");

    // Descriptor heap variant: resources are fetched from the resource and sampler heaps, whose
    // descriptors are 32 bytes apart (matching `styro_rhi::util::DESCRIPTOR_STRIDE`).
    compile(
        shader,
        "mesh_3d_heap",
        &[
            "-DDESCRIPTOR_HEAP",
            "-capability",
            "spvDescriptorHeapEXT",
            "-capability",
            "SPV_EXT_descriptor_heap",
            "-capability",
            "SPV_KHR_untyped_pointers",
            "-spirv-resource-heap-stride",
            "32",
            "-spirv-sampler-heap-stride",
            "32",
        ],
    );

    // Classic bindless variant: unbounded arrays of sampled images in descriptor set 0 and samplers in descriptor set 2
    compile(shader, "mesh_3d_classic", &[]);
}
