use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
    let linker_script = manifest_dir.join("linker.ld");

    println!("cargo:rustc-link-arg=-T{}", linker_script.display());
    println!("cargo:rerun-if-changed={}", linker_script.display());
}
