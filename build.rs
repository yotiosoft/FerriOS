use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    
    let target_dir = out_dir
        .parent().unwrap()  // out
        .parent().unwrap()  // ferrios-xxx
        .parent().unwrap()  // build
        .parent().unwrap()  // debug or release
        .to_path_buf();
    
    // カーネルバイナリ
    let kernel = target_dir.join("ferrios");
    println!("cargo:rerun-if-changed={}", kernel.display());

    if kernel.exists() {
        let bios_path = out_dir.join("bios.img");
        bootloader::BiosBoot::new(&kernel)
            .create_disk_image(&bios_path)
            .unwrap();
        println!("cargo:rustc-env=BIOS_PATH={}", bios_path.display());
    } else {
        eprintln!("Kernel binary not found, skipping disk image creation");
    }

    // テストバイナリ 
    let deps_dir = target_dir.join("deps");
    if let Ok(entries) = std::fs::read_dir(&deps_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("ferrios-")
                        || name.starts_with("basic_boot-")
                        || name.starts_with("heap_allocation-")
                        || name.starts_with("stack_overflow-")
                        || name.starts_with("should_panic-")
                        || name.starts_with("kernel_threads-")
                    {
                        let img_path = out_dir.join(format!("{}.img", name));
                        match bootloader::BiosBoot::new(&path).create_disk_image(&img_path) {
                            Ok(_) => println!("cargo:warning=Created test image: {}", img_path.display()),
                            Err(e) => eprintln!("Failed to create image for {}: {}", name, e),
                        }
                    }
                }
            }
        }
    }
}
