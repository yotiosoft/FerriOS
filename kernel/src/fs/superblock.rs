use super::{
    bcache::{BufferCache, BufferError},
    block::BlockDevice,
    layout::{bitmap_block, bitmap_index_in_block, bitmap_mask, SuperBlock, BPB},
};

pub const SUPERBLOCK_BLOCK: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    Buffer(BufferError),
    InvalidSuperBlock,
    LayoutOverflow,
    BlockOutOfRange,
    DoubleFree,
    NoSpace,
}

impl From<BufferError> for FsError {
    fn from(error: BufferError) -> Self {
        Self::Buffer(error)
    }
}

pub struct FileSystem<'a, D: BlockDevice, const N: usize> {
    cache: &'a BufferCache<D, N>,
    sb: SuperBlock,
}

impl<'a, D: BlockDevice, const N: usize> FileSystem<'a, D, N> {
    pub fn load(cache: &'a BufferCache<D, N>) -> Result<Self, FsError> {
        let sb = read_superblock(cache)?;
        validate_superblock(&sb, cache.device_blocks())?;
        Ok(Self { cache, sb })
    }

    pub fn superblock(&self) -> &SuperBlock {
        &self.sb
    }

    pub fn alloc_block(&self) -> Result<u32, FsError> {
        let end = data_end(&self.sb)?;
        let mut block_no = self.sb.data_start();

        while block_no < end {
            let bitmap_block_no = bitmap_block(block_no, self.sb.ninodes);
            let bitmap_scan_end = end.min(next_bitmap_boundary(block_no)?);
            let mut allocated = None;

            {
                let mut bitmap = self.cache.bread(bitmap_block_no as u64)?;
                while block_no < bitmap_scan_end {
                    let byte_index = bitmap_index_in_block(block_no);
                    let mask = bitmap_mask(block_no);

                    if bitmap.data()[byte_index] & mask == 0 {
                        bitmap.data_mut()[byte_index] |= mask;
                        bitmap.write()?;
                        allocated = Some(block_no);
                        break;
                    }

                    block_no += 1;
                }
            }

            if let Some(allocated) = allocated {
                // xv6 logs this sequence. FerriOS does the same write order now so
                // a future journal can wrap these mutations without changing callers.
                self.zero_block(allocated)?;
                return Ok(allocated);
            }
        }

        Err(FsError::NoSpace)
    }

    pub fn free_block(&self, block_no: u32) -> Result<(), FsError> {
        self.check_data_block(block_no)?;

        let mut bitmap = self
            .cache
            .bread(bitmap_block(block_no, self.sb.ninodes) as u64)?;
        let byte_index = bitmap_index_in_block(block_no);
        let mask = bitmap_mask(block_no);

        if bitmap.data()[byte_index] & mask == 0 {
            return Err(FsError::DoubleFree);
        }

        bitmap.data_mut()[byte_index] &= !mask;
        bitmap.write()?;
        Ok(())
    }

    pub fn zero_block(&self, block_no: u32) -> Result<(), FsError> {
        self.check_data_block(block_no)?;

        let mut block = self.cache.bread(block_no as u64)?;
        block.data_mut().fill(0);
        block.write()?;
        Ok(())
    }

    fn check_data_block(&self, block_no: u32) -> Result<(), FsError> {
        if block_no < self.sb.data_start() || block_no >= data_end(&self.sb)? {
            return Err(FsError::BlockOutOfRange);
        }
        Ok(())
    }
}

pub fn read_superblock<D: BlockDevice, const N: usize>(
    cache: &BufferCache<D, N>,
) -> Result<SuperBlock, FsError> {
    let block = cache.bread(SUPERBLOCK_BLOCK)?;
    Ok(SuperBlock::from_block_bytes(block.data()))
}

pub fn validate_superblock(sb: &SuperBlock, device_blocks: u64) -> Result<(), FsError> {
    if sb.size == 0 || sb.size as u64 > device_blocks || sb.ninodes == 0 || sb.nblocks == 0 {
        return Err(FsError::InvalidSuperBlock);
    }

    let inode_start = sb.inode_start();
    let inode_end = checked_add(inode_start, sb.inode_blocks())?;
    let bitmap_start = sb.bitmap_start();
    let bitmap_end = checked_add(bitmap_start, sb.bitmap_blocks())?;
    let data_start = bitmap_end;
    let data_end = checked_add(data_start, sb.nblocks)?;

    if inode_start >= sb.size
        || bitmap_start >= sb.size
        || data_start > sb.size
        || data_end > sb.size
        || inode_end > bitmap_start
        || bitmap_end > data_start
        || data_start != bitmap_end
    {
        return Err(FsError::InvalidSuperBlock);
    }

    let bitmap_capacity = checked_mul(sb.bitmap_blocks(), BPB as u32)?;
    if bitmap_capacity < sb.size || checked_sub(data_end, data_start)? != sb.nblocks {
        return Err(FsError::InvalidSuperBlock);
    }

    Ok(())
}

fn data_end(sb: &SuperBlock) -> Result<u32, FsError> {
    checked_add(sb.data_start(), sb.nblocks)
}

fn next_bitmap_boundary(block_no: u32) -> Result<u32, FsError> {
    checked_add(block_no / BPB as u32, 1).and_then(|chunk| checked_mul(chunk, BPB as u32))
}

fn checked_add(left: u32, right: u32) -> Result<u32, FsError> {
    left.checked_add(right).ok_or(FsError::LayoutOverflow)
}

fn checked_sub(left: u32, right: u32) -> Result<u32, FsError> {
    left.checked_sub(right).ok_or(FsError::LayoutOverflow)
}

fn checked_mul(left: u32, right: u32) -> Result<u32, FsError> {
    left.checked_mul(right).ok_or(FsError::LayoutOverflow)
}

#[cfg(test)]
mod tests {
    use spin::Mutex;

    use super::*;
    use crate::fs::block::{BlockError, BLOCK_SIZE};

    const TEST_BLOCKS: usize = 64;
    const TEST_NINODES: u32 = 16;
    const TEST_NBLOCKS: u32 = 54;
    const TEST_DATA_START: u32 = 6;

    struct TestDevice<const B: usize> {
        blocks: Mutex<[[u8; BLOCK_SIZE]; B]>,
    }

    impl<const B: usize> TestDevice<B> {
        fn new() -> Self {
            Self {
                blocks: Mutex::new([[0; BLOCK_SIZE]; B]),
            }
        }
    }

    impl<const B: usize> BlockDevice for TestDevice<B> {
        fn num_blocks(&self) -> u64 {
            B as u64
        }

        fn read_block(&self, block_no: u64, dst: &mut [u8; BLOCK_SIZE]) -> Result<(), BlockError> {
            let blocks = self.blocks.lock();
            let src = blocks
                .get(block_no as usize)
                .ok_or(BlockError::OutOfRange)?;
            dst.copy_from_slice(src);
            Ok(())
        }

        fn write_block(&self, block_no: u64, src: &[u8; BLOCK_SIZE]) -> Result<(), BlockError> {
            let mut blocks = self.blocks.lock();
            let dst = blocks
                .get_mut(block_no as usize)
                .ok_or(BlockError::OutOfRange)?;
            dst.copy_from_slice(src);
            Ok(())
        }
    }

    fn test_superblock() -> SuperBlock {
        SuperBlock {
            size: TEST_BLOCKS as u32,
            nblocks: TEST_NBLOCKS,
            ninodes: TEST_NINODES,
            nlog: 0,
        }
    }

    fn write_superblock(block: &mut [u8; BLOCK_SIZE], sb: SuperBlock) {
        block[0..4].copy_from_slice(&sb.size.to_le_bytes());
        block[4..8].copy_from_slice(&sb.nblocks.to_le_bytes());
        block[8..12].copy_from_slice(&sb.ninodes.to_le_bytes());
        block[12..16].copy_from_slice(&sb.nlog.to_le_bytes());
    }

    fn mark_used(blocks: &mut [[u8; BLOCK_SIZE]; TEST_BLOCKS], sb: SuperBlock, end: u32) {
        for block_no in 0..end {
            let bitmap_block_no = bitmap_block(block_no, sb.ninodes) as usize;
            blocks[bitmap_block_no][bitmap_index_in_block(block_no)] |= bitmap_mask(block_no);
        }
    }

    fn bitmap_used<const B: usize>(cache: &BufferCache<TestDevice<B>, 8>, block_no: u32) -> bool {
        let fs = FileSystem::load(cache).expect("load fs");
        let bitmap = cache
            .bread(bitmap_block(block_no, fs.superblock().ninodes) as u64)
            .expect("read bitmap");
        bitmap.data()[bitmap_index_in_block(block_no)] & bitmap_mask(block_no) != 0
    }

    fn new_cache() -> BufferCache<TestDevice<TEST_BLOCKS>, 8> {
        let device = TestDevice::<TEST_BLOCKS>::new();
        {
            let mut blocks = device.blocks.lock();
            let sb = test_superblock();
            write_superblock(&mut blocks[SUPERBLOCK_BLOCK as usize], sb);
            mark_used(&mut blocks, sb, TEST_DATA_START + 1);
            blocks[(TEST_DATA_START + 1) as usize][7] = 0xa5;
        }
        BufferCache::new(device)
    }

    #[test_case]
    fn reads_valid_superblock() {
        let cache = new_cache();
        let fs = FileSystem::load(&cache).expect("load filesystem");

        assert_eq!(*fs.superblock(), test_superblock());
    }

    #[test_case]
    fn rejects_corrupt_superblock() {
        let device = TestDevice::<TEST_BLOCKS>::new();
        write_superblock(
            &mut device.blocks.lock()[SUPERBLOCK_BLOCK as usize],
            SuperBlock {
                size: (TEST_BLOCKS + 1) as u32,
                ..test_superblock()
            },
        );
        let cache = BufferCache::<_, 8>::new(device);

        assert_eq!(
            FileSystem::load(&cache).map(|_| ()),
            Err(FsError::InvalidSuperBlock)
        );
    }

    #[test_case]
    fn alloc_block_returns_free_data_block_and_marks_bitmap() {
        let cache = new_cache();
        let fs = FileSystem::load(&cache).expect("load fs");

        let block_no = fs.alloc_block().expect("allocate block");

        assert_eq!(block_no, TEST_DATA_START + 1);
        assert!(bitmap_used(&cache, block_no));
    }

    #[test_case]
    fn allocated_block_is_zeroed() {
        let cache = new_cache();
        let fs = FileSystem::load(&cache).expect("load fs");

        let block_no = fs.alloc_block().expect("allocate block");
        let block = cache.bread(block_no as u64).expect("read allocated block");

        assert_eq!(block.data(), &[0u8; BLOCK_SIZE]);
    }

    #[test_case]
    fn repeated_allocations_return_distinct_blocks() {
        let cache = new_cache();
        let fs = FileSystem::load(&cache).expect("load fs");

        let first = fs.alloc_block().expect("allocate first block");
        let second = fs.alloc_block().expect("allocate second block");

        assert_ne!(first, second);
    }

    #[test_case]
    fn free_block_clears_bitmap_and_can_be_reallocated() {
        let cache = new_cache();
        let fs = FileSystem::load(&cache).expect("load fs");
        let block_no = fs.alloc_block().expect("allocate block");

        fs.free_block(block_no).expect("free block");
        assert!(!bitmap_used(&cache, block_no));

        assert_eq!(fs.alloc_block().expect("allocate again"), block_no);
    }

    #[test_case]
    fn double_free_is_reported() {
        let cache = new_cache();
        let fs = FileSystem::load(&cache).expect("load fs");
        let block_no = fs.alloc_block().expect("allocate block");

        fs.free_block(block_no).expect("free block");

        assert_eq!(fs.free_block(block_no), Err(FsError::DoubleFree));
    }

    #[test_case]
    fn out_of_range_free_is_reported() {
        let cache = new_cache();
        let fs = FileSystem::load(&cache).expect("load fs");

        assert_eq!(
            fs.free_block(TEST_DATA_START - 1),
            Err(FsError::BlockOutOfRange)
        );
        assert_eq!(
            fs.free_block(TEST_DATA_START + TEST_NBLOCKS),
            Err(FsError::BlockOutOfRange)
        );
    }

    #[test_case]
    fn no_space_is_reported() {
        let device = TestDevice::<TEST_BLOCKS>::new();
        {
            let mut blocks = device.blocks.lock();
            let sb = test_superblock();
            write_superblock(&mut blocks[SUPERBLOCK_BLOCK as usize], sb);
            mark_used(&mut blocks, sb, TEST_DATA_START + TEST_NBLOCKS);
        }
        let cache = BufferCache::<_, 8>::new(device);
        let fs = FileSystem::load(&cache).expect("load fs");

        assert_eq!(fs.alloc_block(), Err(FsError::NoSpace));
    }
}
