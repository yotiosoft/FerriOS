pub mod bcache;
pub mod block;
pub mod layout;
pub mod superblock;

pub use superblock::{read_superblock, validate_superblock, FileSystem, FsError};

pub fn init() -> Result<(), bcache::BufferError> {
    bcache::init()
}
