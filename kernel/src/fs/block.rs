pub const BLOCK_SIZE: usize = 512;

pub trait BlockDevice {
    fn num_blocks(&self) -> u64;
    fn read_block(&self, block_no: u64, dst: &mut [u8; BLOCK_SIZE]) -> Result<(), BlockError>;
    fn write_block(&self, block_no: u64, src: &mut [u8; BLOCK_SIZE]) -> Result<(), BlockError>;
}
