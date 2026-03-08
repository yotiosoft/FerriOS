use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    
    let target_dir = out_dir
        .parent().unwrap()
        .parent().unwrap()
        .parent().unwrap()
        .to_path_buf();
    
    let kernel = target_dir.join("ferrios");
    
    println!("cargo:rerun-if-changed={}", kernel.display());

    if !kernel.exists() {
        eprintln!("Kernel binary not found, skipping disk image creation");
        return;
    }

    let bios_path = out_dir.join("bios.img");
    bootloader::BiosBoot::new(&kernel)
        .create_disk_image(&bios_path)
        .unwrap();

    println!("cargo:rustc-env=BIOS_PATH={}", bios_path.display());
}
