use std::fs;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set"));
    let apps_manifest = PathBuf::from(
        std::env::var("USER_APPS_MANIFEST").expect("USER_APPS_MANIFEST not set"),
    );

    let manifest_text = fs::read_to_string(&apps_manifest).expect("failed to read apps manifest");
    let mut generated = String::from("pub static PROGRAMS: &[UserProgram] = &[\n");

    for line in manifest_text.lines().filter(|line| !line.trim().is_empty()) {
        let mut parts = line.splitn(3, '\t');
        let path = parts.next().expect("missing runtime path");
        let file_stem = parts.next().expect("missing app file stem");
        let elf_src = PathBuf::from(parts.next().expect("missing elf path"));

        let dst = out_dir.join(format!("{file_stem}.elf"));
        fs::copy(&elf_src, &dst).expect("failed to copy app elf");
        println!("cargo:rerun-if-changed={}", elf_src.display());

        generated.push_str(&format!(
            "    UserProgram {{ path: {:?}, elf: include_bytes!(concat!(env!(\"OUT_DIR\"), \"/{}.elf\")) }},\n",
            path,
            file_stem,
        ));
    }

    generated.push_str("];\n");
    fs::write(out_dir.join("user_programs.rs"), generated).expect("failed to write generated user programs");

    println!("cargo:rerun-if-env-changed=USER_APPS_MANIFEST");
    println!("cargo:rerun-if-changed={}", apps_manifest.display());
}
