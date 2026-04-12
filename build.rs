use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let kernel_manifest = PathBuf::from(&manifest_dir).join("kernel/Cargo.toml");
    let user_dir = PathBuf::from(&manifest_dir).join("user");
    let target_dir = PathBuf::from(&manifest_dir).join("target/kernel-build");
    let user_target_dir = PathBuf::from(&manifest_dir).join("target/user-build");

    // --release かどうか検知
    let profile = std::env::var("PROFILE").unwrap();
    let is_release = profile == "release";
    let profile_dir = if is_release { "release" } else { "debug" };

    // user ELF を先にビルド
    let mut user_args = vec![
        "+nightly".to_string(),
        "-Zjson-target-spec".to_string(),
        "build".to_string(),
        "--target-dir".to_string(),
        user_target_dir.to_str().unwrap().to_string(),
    ];
    if is_release {
        user_args.push("--release".to_string());
    }

    let user_status = Command::new("cargo")
        .current_dir(&user_dir)
        .env_remove("CARGO_TARGET_DIR")
        .args(&user_args)
        .status()
        .expect("failed to build user programs");
    assert!(user_status.success(), "user build failed");

    let user_init_elf = user_target_dir.join(format!("x86_64-ferrios/{profile_dir}/user"));

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
        .env("USER_INIT_ELF", &user_init_elf)
        .env_remove("CARGO_TARGET_DIR")
        .args(&args)
        .status()
        .expect("failed to build kernel");

    assert!(status.success(), "kernel build failed");

    let kernel = target_dir
        .join(format!("x86_64-unknown-none/{profile_dir}/ferrios"));

    println!("cargo:rerun-if-changed={}", kernel_manifest.display());
    println!("cargo:rerun-if-changed=kernel/src");
    println!("cargo:rerun-if-changed=abi/src");
    println!("cargo:rerun-if-changed=abi/Cargo.toml");
    println!("cargo:rerun-if-changed=user/src");
    println!("cargo:rerun-if-changed=user/linker.ld");
    println!("cargo:rerun-if-changed=user/build.rs");
    println!("cargo:rerun-if-changed=user/Cargo.toml");
    println!("cargo:rerun-if-changed=user/.cargo/config.toml");
    println!("cargo:rerun-if-changed=userlib/src");
    println!("cargo:rerun-if-changed=userlib/Cargo.toml");
    println!("cargo:rerun-if-changed=x86_64-ferrios.json");

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let bios_path = out_dir.join("bios.img");

    bootloader::BiosBoot::new(&kernel)
        .create_disk_image(&bios_path)
        .unwrap();

    println!("cargo:rustc-env=BIOS_PATH={}", bios_path.display());
}
