use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=tests/ffi_smoke.c");
    println!("cargo:rerun-if-changed=include/m2_ffi.h");
    if env::var_os("CARGO_FEATURE_C_ABI_SMOKE").is_none()
        || env::var("TARGET").ok() != env::var("HOST").ok()
    {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set"));
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir is set"));
    let library = out.join("libxiaohong_rag.so");
    let executable = out.join("ffi_smoke");
    let status = Command::new(env::var_os("RUSTC").expect("RUSTC is set"))
        .args(["--crate-name", "xiaohong_rag", "--crate-type", "cdylib", "--edition=2024"])
        .arg(manifest.join("src/lib.rs"))
        .arg("-o").arg(&library)
        .status().expect("failed to build C ABI cdylib");
    assert!(status.success(), "building the C ABI cdylib failed");
    let status = Command::new("cc")
        .arg("-std=c11").arg("-Wall").arg("-Wextra").arg("-Werror")
        .arg("-I").arg(manifest.join("include"))
        .arg(manifest.join("tests/ffi_smoke.c"))
        .arg("-L").arg(&out).arg("-lxiaohong_rag")
        .arg(format!("-Wl,-rpath,{}", out.display()))
        .arg("-o").arg(&executable)
        .status().expect("failed to compile the C ABI smoke test (install a C compiler)");
    assert!(status.success(), "compiling the C ABI smoke test failed");
    let status = Command::new(executable).status().expect("failed to run the C ABI smoke test");
    assert!(status.success(), "C ABI smoke test failed: {status}");
}