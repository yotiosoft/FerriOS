mod ramdisk;

pub const BLOCK_SIZE: usize = 512;

pub fn block_size() -> usize {
    BLOCK_SIZE
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockError {
    InvalidImageSize,
    OutOfRange,
    NotInitialized,
    AlreadyInitialized,
}

pub trait BlockDevice {
    fn num_blocks(&self) -> u64;
    fn read_block(&self, block_no: u64, dst: &mut [u8; BLOCK_SIZE]) -> Result<(), BlockError>;
    fn write_block(&self, block_no: u64, src: &[u8; BLOCK_SIZE]) -> Result<(), BlockError>;
}

static ROOT_BLOCK_DEVICE: spin::Mutex<Option<ramdisk::RamBlockDevice<'static>>> =
    spin::Mutex::new(None);

pub fn init_root_device(image: &'static mut [u8]) -> Result<(), BlockError> {
    let device = ramdisk::RamBlockDevice::new(image)?;
    let mut root_device = ROOT_BLOCK_DEVICE.lock();

    if root_device.is_some() {
        return Err(BlockError::AlreadyInitialized);
    }

    *root_device = Some(device);
    Ok(())
}

pub fn root_num_blocks() -> Result<u64, BlockError> {
    let root_device = ROOT_BLOCK_DEVICE.lock();
    let device = root_device.as_ref().ok_or(BlockError::NotInitialized)?;

    Ok(device.num_blocks())
}

pub fn read_root_block(block_no: u64, dst: &mut [u8; BLOCK_SIZE]) -> Result<(), BlockError> {
    let root_device = ROOT_BLOCK_DEVICE.lock();
    let device = root_device.as_ref().ok_or(BlockError::NotInitialized)?;

    device.read_block(block_no, dst)
}

pub fn write_root_block(block_no: u64, src: &[u8; BLOCK_SIZE]) -> Result<(), BlockError> {
    let root_device = ROOT_BLOCK_DEVICE.lock();
    let device = root_device.as_ref().ok_or(BlockError::NotInitialized)?;

    device.write_block(block_no, src)
}
