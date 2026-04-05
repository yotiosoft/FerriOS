use std::fs;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set"));
    let user_init_elf = PathBuf::from(
        std::env::var("USER_INIT_ELF").expect("USER_INIT_ELF not set"),
    );

    let dst = out_dir.join("init.elf");
    fs::copy(&user_init_elf, &dst).expect("failed to copy user init elf");

    println!("cargo:rerun-if-env-changed=USER_INIT_ELF");
    println!("cargo:rerun-if-changed={}", user_init_elf.display());
}
