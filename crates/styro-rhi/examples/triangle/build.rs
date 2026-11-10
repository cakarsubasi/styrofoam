use std::{env, path::PathBuf, process::Command};

fn main() {
    let shader = "src/triangle.slang";
    println!("cargo:rerun-if-changed={shader}");
    println!("cargo:rerun-if-changed=build.rs");

    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("triangle.spv");

    // Note that most of these are not needed since we don't have images on this example
    // but it indicates the expected usage for slang.
    let status = Command::new("slangc")
        .arg(shader)
        .args(["-target", "spirv"])
        .arg("-fvk-use-entrypoint-name")
        .args(["-capability", "spvDescriptorHeapEXT"])
        .args(["-capability", "SPV_EXT_descriptor_heap"])
        .args(["-capability", "SPV_KHR_untyped_pointers"])
        .args(["-spirv-resource-heap-stride", "32"])
        .arg("-o")
        .arg(&out)
        .status()
        .expect("failed to run `slangc`; install the Vulkan SDK and make sure it is on PATH");

    assert!(status.success(), "slangc failed to compile {shader}");
}
