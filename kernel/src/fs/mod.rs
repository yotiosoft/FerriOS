pub mod block;
pub mod bcache;
pub mod layout;

pub fn init() -> Result<(), bcache::BufferError> {
    bcache::init()
}
