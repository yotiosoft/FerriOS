use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let kernel_manifest = PathBuf::from(&manifest_dir).join("kernel/Cargo.toml");
    let target_dir = PathBuf::from(&manifest_dir).join("target/kernel-build");

    // --release かどうか検知
    let profile = std::env::var("PROFILE").unwrap();
    let is_release = profile == "release";

    let mut args = vec![
        "+nightly".to_string(),
        "build".to_string(),
        "--manifest-path".to_string(),
        kernel_manifest.to_str().unwrap().to_string(),
        "--target".to_string(),
        "x86_64-unknown-none".to_string(),
        "-Zbuild-std=core,compiler_builtins,alloc".to_string(),
        "-Zbuild-std-features=compiler-builtins-mem".to_string(),
        "--target-dir".to_string(),
        target_dir.to_str().unwrap().to_string(),
    ];
    if is_release {
        args.push("--release".to_string());
    }

    let status = Command::new("cargo")
        .args(&args)
        .status()
        .expect("failed to build kernel");

    assert!(status.success(), "kernel build failed");

    // debug or release
    let profile_dir = if is_release { "release" } else { "debug" };
    let kernel = target_dir
        .join(format!("x86_64-unknown-none/{profile_dir}/ferrios"));

    println!("cargo:rerun-if-changed={}", kernel_manifest.display());
    println!("cargo:rerun-if-changed=kernel/src");

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let bios_path = out_dir.join("bios.img");

    bootloader::BiosBoot::new(&kernel)
        .create_disk_image(&bios_path)
        .unwrap();

    println!("cargo:rustc-env=BIOS_PATH={}", bios_path.display());
}
