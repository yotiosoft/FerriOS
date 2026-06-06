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

pub struct RamBlockDevice<'a> {
    image: spin::Mutex<&'a mut [u8]>,
}

impl<'a> RamBlockDevice<'a> {
    pub fn new(image: &'a mut [u8]) -> Result<Self, BlockError> {
        if image.len() % BLOCK_SIZE != 0 {
            return Err(BlockError::InvalidImageSize);
        }

        Ok(Self {
            image: spin::Mutex::new(image),
        })
    }

    fn block_range(block_no: u64, image_len: usize) -> Result<core::ops::Range<usize>, BlockError> {
        let num_blocks = image_len / BLOCK_SIZE;

        if block_no >= num_blocks as u64 {
            return Err(BlockError::OutOfRange);
        }

        let start = block_no as usize * BLOCK_SIZE;
        Ok(start..start + BLOCK_SIZE)
    }
}

impl BlockDevice for RamBlockDevice<'_> {
    fn num_blocks(&self) -> u64 {
        (self.image.lock().len() / BLOCK_SIZE) as u64
    }

    fn read_block(&self, block_no: u64, dst: &mut [u8; BLOCK_SIZE]) -> Result<(), BlockError> {
        let image = self.image.lock();
        let range = Self::block_range(block_no, image.len())?;

        dst.copy_from_slice(&image[range]);
        Ok(())
    }

    fn write_block(&self, block_no: u64, src: &[u8; BLOCK_SIZE]) -> Result<(), BlockError> {
        let mut image = self.image.lock();
        let range = Self::block_range(block_no, image.len())?;

        image[range].copy_from_slice(src);
        Ok(())
    }
}

static ROOT_BLOCK_DEVICE: spin::Mutex<Option<RamBlockDevice<'static>>> = spin::Mutex::new(None);

pub fn init_root_device(image: &'static mut [u8]) -> Result<(), BlockError> {
    let device = RamBlockDevice::new(image)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn ram_block_device_reads_written_block() {
        let mut image = [0u8; BLOCK_SIZE];
        let device = RamBlockDevice::new(&mut image).expect("valid RAM disk image");
        let mut src = [0u8; BLOCK_SIZE];
        let mut dst = [0u8; BLOCK_SIZE];

        for (i, byte) in src.iter_mut().enumerate() {
            *byte = (i % 251) as u8;
        }

        device.write_block(0, &src).expect("write block 0");
        device.read_block(0, &mut dst).expect("read block 0");

        assert_eq!(src, dst);
    }

    #[test_case]
    fn ram_block_device_keeps_blocks_independent() {
        let mut image = [0u8; BLOCK_SIZE * 2];
        let device = RamBlockDevice::new(&mut image).expect("valid RAM disk image");
        let first = [0x11u8; BLOCK_SIZE];
        let second = [0x22u8; BLOCK_SIZE];
        let mut dst = [0u8; BLOCK_SIZE];

        device.write_block(0, &first).expect("write block 0");
        device.write_block(1, &second).expect("write block 1");

        device.read_block(0, &mut dst).expect("read block 0");
        assert_eq!(first, dst);

        device.read_block(1, &mut dst).expect("read block 1");
        assert_eq!(second, dst);
    }

    #[test_case]
    fn ram_block_device_reports_out_of_range() {
        let mut image = [0u8; BLOCK_SIZE];
        let device = RamBlockDevice::new(&mut image).expect("valid RAM disk image");
        let mut dst = [0u8; BLOCK_SIZE];
        let src = [0u8; BLOCK_SIZE];

        assert_eq!(device.read_block(1, &mut dst), Err(BlockError::OutOfRange));
        assert_eq!(device.write_block(1, &src), Err(BlockError::OutOfRange));
    }

    #[test_case]
    fn ram_block_device_rejects_invalid_image_size() {
        let mut image = [0u8; BLOCK_SIZE + 1];

        assert_eq!(
            RamBlockDevice::new(&mut image).map(|_| ()),
            Err(BlockError::InvalidImageSize)
        );
    }
}
