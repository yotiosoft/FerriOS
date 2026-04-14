use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct AppBuild {
    dir_name: String,
    package_name: String,
    manifest_path: PathBuf,
}

fn parse_package_name(manifest_path: &Path) -> String {
    let manifest = fs::read_to_string(manifest_path).expect("failed to read app Cargo.toml");
    manifest
        .lines()
        .find_map(|line| {
            let line = line.trim();
            line.strip_prefix("name = ")
                .map(|value| value.trim_matches('"').to_string())
        })
        .expect("app package name not found")
}

fn discover_apps(apps_root: &Path) -> Vec<AppBuild> {
    let mut apps = Vec::new();
    let entries = fs::read_dir(apps_root).expect("failed to read apps directory");

    for entry in entries {
        let entry = entry.expect("failed to read apps entry");
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("Cargo.toml");
        if !manifest_path.exists() {
            continue;
        }

        let dir_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("invalid app directory name")
            .to_string();
        let package_name = parse_package_name(&manifest_path);

        apps.push(AppBuild {
            dir_name,
            package_name,
            manifest_path,
        });
    }

    apps.sort_by(|a, b| a.dir_name.cmp(&b.dir_name));
    apps
}

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let kernel_manifest = PathBuf::from(&manifest_dir).join("kernel/Cargo.toml");
    let apps_root = PathBuf::from(&manifest_dir).join("apps");
    let app_target = PathBuf::from(&manifest_dir).join("x86_64-ferrios.json");
    let target_dir = PathBuf::from(&manifest_dir).join("target/kernel-build");
    let apps_target_dir = PathBuf::from(&manifest_dir).join("target/apps-build");
    let apps = discover_apps(&apps_root);
    assert!(!apps.is_empty(), "no app crates found under apps/");

    // --release かどうか検知
    let profile = std::env::var("PROFILE").unwrap();
    let is_release = profile == "release";
    let profile_dir = if is_release { "release" } else { "debug" };

    let mut manifest_lines = Vec::new();
    let has_init = apps.iter().any(|app| app.dir_name == "init");

    // app ELF を先にビルド
    for app in &apps {
        let mut app_args = vec![
            "+nightly".to_string(),
            "-Zjson-target-spec".to_string(),
            "build".to_string(),
            "--manifest-path".to_string(),
            app.manifest_path.to_str().unwrap().to_string(),
            "--target".to_string(),
            app_target.to_str().unwrap().to_string(),
            "--target-dir".to_string(),
            apps_target_dir.to_str().unwrap().to_string(),
        ];
        if is_release {
            app_args.push("--release".to_string());
        }

        let app_status = Command::new("cargo")
            .env_remove("CARGO_TARGET_DIR")
            .args(&app_args)
            .status()
            .expect("failed to build app");
        assert!(app_status.success(), "app build failed: {}", app.dir_name);

        let elf_path = apps_target_dir.join(format!(
            "x86_64-ferrios/{profile_dir}/{}",
            app.package_name
        ));
        let runtime_path = format!("/{}", app.dir_name);
        manifest_lines.push(format!(
            "{}\t{}\t{}",
            runtime_path,
            app.package_name,
            elf_path.display()
        ));
    }

    if !has_init {
        let default_app = &apps[0];
        let elf_path = apps_target_dir.join(format!(
            "x86_64-ferrios/{profile_dir}/{}",
            default_app.package_name
        ));
        manifest_lines.push(format!(
            "/init\t{}\t{}",
            default_app.package_name,
            elf_path.display()
        ));
    }

    fs::create_dir_all(&apps_target_dir).expect("failed to create apps target dir");
    let apps_manifest = apps_target_dir.join("user_programs_manifest.tsv");
    fs::write(&apps_manifest, manifest_lines.join("\n")).expect("failed to write apps manifest");

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
        .env("USER_APPS_MANIFEST", &apps_manifest)
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
    println!("cargo:rerun-if-changed=apps");
    for app in &apps {
        let app_dir = app
            .manifest_path
            .parent()
            .expect("app manifest does not have parent");
        println!("cargo:rerun-if-changed={}", app_dir.join("src").display());
        println!("cargo:rerun-if-changed={}", app_dir.join("linker.ld").display());
        println!("cargo:rerun-if-changed={}", app_dir.join("build.rs").display());
        println!("cargo:rerun-if-changed={}", app.manifest_path.display());
        println!(
            "cargo:rerun-if-changed={}",
            app_dir.join(".cargo/config.toml").display()
        );
    }
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
