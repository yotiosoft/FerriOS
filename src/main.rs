use std::process::Command;

fn main() {
    let bios_path = std::env::var("BIOS_PATH")
        .expect("BIOS_PATH not set – run via `cargo run`");

    let nographic = std::env::args().any(|a| a == "--nographic");

    let mut args = vec![
        "-drive".to_string(), format!("format=raw,file={bios_path}"),
    ];
    if nographic {
        args.push("-nographic".to_string());
    }
    else {
        args.push("-serial".to_string());
        args.push("stdio".to_string());
    }

    let exit_status = Command::new("qemu-system-x86_64")
        .args(&args)
        .status()
        .expect("failed to execute QEMU");

    if !exit_status.success() {
        std::process::exit(exit_status.code().unwrap_or(1));
    }
}
