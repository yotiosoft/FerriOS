use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

use ferrios_mkfs::{MkfsBuilder, MkfsConfig};

const DEFAULT_SIZE: u32 = 1_000;
const DEFAULT_NINODES: u32 = 200;
const BLOCK_SIZE: u32 = 512;
const DINODE_SIZE: u32 = 64;
const BPB: u32 = BLOCK_SIZE * 8;
const IPB: u32 = BLOCK_SIZE / DINODE_SIZE;

fn main() {
    if let Err(error) = run(env::args_os().collect()) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run(args: Vec<OsString>) -> Result<(), String> {
    if args.len() < 2 {
        return Err(format!(
            "usage: {} <output fs.img> <input files...>",
            args.first()
                .and_then(|arg| arg.to_str())
                .unwrap_or("ferrios-mkfs")
        ));
    }

    let output = PathBuf::from(&args[1]);
    let mut builder = MkfsBuilder::new(default_config());

    for input in &args[2..] {
        let path = PathBuf::from(input);
        let metadata =
            fs::metadata(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        if !metadata.is_file() {
            return Err(format!("{}: input must be a regular file", path.display()));
        }

        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("{}: file name must be valid UTF-8", path.display()))?;
        let content = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        builder
            .add_file(name, content)
            .map_err(|error| format!("{}: {error:?}", path.display()))?;
    }

    let image = builder
        .build()
        .map_err(|error| format!("mkfs failed: {error:?}"))?;
    fs::write(&output, image).map_err(|error| format!("{}: {error}", output.display()))?;

    Ok(())
}

fn default_config() -> MkfsConfig {
    let inode_blocks = DEFAULT_NINODES / IPB + 1;
    let bitmap_blocks = DEFAULT_SIZE.div_ceil(BPB);
    let metadata_blocks = 2 + inode_blocks + bitmap_blocks;

    MkfsConfig {
        size: DEFAULT_SIZE,
        nblocks: DEFAULT_SIZE - metadata_blocks,
        ninodes: DEFAULT_NINODES,
        nlog: 0,
    }
}
