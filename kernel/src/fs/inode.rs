use core::{
    array,
    ops::{Deref, DerefMut},
};

use spin::{Mutex, MutexGuard};

use super::{
    bcache::BufferCache,
    block::{BLOCK_SIZE, BlockDevice},
    layout::{
        DiskInode, MAXFILE, NDIRECT, NINDIRECT, SuperBlock, T_DEV, T_DIR, T_FILE, T_NONE,
        inode_block, inode_index_in_block,
    },
    superblock::FsError,
};

pub const NINODE: usize = 50;

#[derive(Clone, Copy)]
struct EntryMeta {
    inum: u32,
    refcnt: usize,
}

impl EntryMeta {
    const EMPTY: Self = Self { inum: 0, refcnt: 0 };
}

/// The in-memory portion of an inode. It is protected independently from the
/// cache metadata, so disk I/O never runs while the metadata lock is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Inode {
    pub valid: bool,
    pub type_: u16,
    pub major: u16,
    pub minor: u16,
    pub nlink: u16,
    pub size: u32,
    pub addrs: [u32; NDIRECT + 3],
}

impl Inode {
    const EMPTY: Self = Self {
        valid: false,
        type_: T_NONE,
        major: 0,
        minor: 0,
        nlink: 0,
        size: 0,
        addrs: [0; NDIRECT + 3],
    };

    fn load(&mut self, disk: DiskInode) {
        self.type_ = disk.type_;
        self.major = disk.major;
        self.minor = disk.minor;
        self.nlink = disk.nlink;
        self.size = disk.size;
        self.addrs = disk.addrs;
        self.valid = true;
    }

    fn to_disk(self) -> DiskInode {
        DiskInode {
            type_: self.type_,
            major: self.major,
            minor: self.minor,
            nlink: self.nlink,
            size: self.size,
            addrs: self.addrs,
        }
    }
}

struct CacheEntry {
    inode: Mutex<Inode>,
}

/// Fixed-size inode cache.
///
/// Lock order is cache metadata, then an inode entry, then the buffer cache.
/// No operation holds the metadata lock while acquiring either later lock.
pub struct InodeCache<'a, D: BlockDevice, const B: usize, const I: usize = NINODE> {
    buffers: &'a BufferCache<D, B>,
    sb: SuperBlock,
    metadata: Mutex<[EntryMeta; I]>,
    entries: [CacheEntry; I],
    allocation: Mutex<()>,
}

impl<'a, D: BlockDevice, const B: usize, const I: usize> InodeCache<'a, D, B, I> {
    pub fn new(buffers: &'a BufferCache<D, B>, sb: SuperBlock) -> Self {
        Self {
            buffers,
            sb,
            metadata: Mutex::new([EntryMeta::EMPTY; I]),
            entries: array::from_fn(|_| CacheEntry {
                inode: Mutex::new(Inode::EMPTY),
            }),
            allocation: Mutex::new(()),
        }
    }

    /// `iget`: acquire a cache reference without reading the inode block.
    pub fn iget(&self, inum: u32) -> Result<InodeRef<'_, 'a, D, B, I>, FsError> {
        self.check_inum(inum)?;

        let index = {
            let mut metadata = self.metadata.lock();
            if let Some(index) = metadata
                .iter()
                .position(|entry| entry.refcnt > 0 && entry.inum == inum)
            {
                metadata[index].refcnt = metadata[index]
                    .refcnt
                    .checked_add(1)
                    .ok_or(FsError::InodeRefOverflow)?;
                index
            } else {
                let index = metadata
                    .iter()
                    .position(|entry| entry.refcnt == 0)
                    .ok_or(FsError::NoFreeInode)?;
                metadata[index] = EntryMeta { inum, refcnt: 1 };
                // Reset stale contents while following metadata -> entry order.
                *self.entries[index].inode.lock() = Inode::EMPTY;
                index
            }
        };

        Ok(InodeRef {
            cache: self,
            index,
            inum,
            live: true,
        })
    }

    /// `ialloc`: allocate an on-disk inode and return it unlocked.
    pub fn ialloc(&self, type_: u16) -> Result<InodeRef<'_, 'a, D, B, I>, FsError> {
        if type_ == T_NONE {
            return Err(FsError::InvalidInodeType);
        }

        // Serialize table scanning so two allocators cannot claim one slot.
        let _allocation = self.allocation.lock();
        for inum in 1..self.sb.ninodes {
            let mut block = self.buffers.bread(inode_block(inum) as u64)?;
            let range = inode_byte_range(inum)?;
            let disk = DiskInode::decode_from(&block.data()[range.clone()])
                .ok_or(FsError::CorruptImage)?;
            if disk.type_ != T_NONE {
                continue;
            }

            let mut allocated = DiskInode::empty();
            allocated.type_ = type_;
            if !allocated.encode_into(&mut block.data_mut()[range]) {
                return Err(FsError::CorruptImage);
            }
            block.write()?;
            drop(block);
            return self.iget(inum);
        }

        Err(FsError::NoFreeInode)
    }

    fn check_inum(&self, inum: u32) -> Result<(), FsError> {
        if inum == 0 {
            Err(FsError::InvalidInode)
        } else if inum >= self.sb.ninodes {
            Err(FsError::InodeOutOfRange)
        } else {
            Ok(())
        }
    }

    fn release(&self, index: usize, inum: u32) {
        let mut metadata = self.metadata.lock();
        if let Some(entry) = metadata.get_mut(index) {
            if entry.inum == inum && entry.refcnt > 0 {
                entry.refcnt -= 1;
                // A future itrunc will explicitly handle nlink == 0 before this
                // final reference is released; Drop must never perform I/O.
            }
        }
    }

    #[cfg(test)]
    fn refcnt(&self, inum: u32) -> usize {
        self.metadata
            .lock()
            .iter()
            .find(|entry| entry.inum == inum)
            .map_or(0, |entry| entry.refcnt)
    }
}

pub struct InodeRef<'cache, 'device, D: BlockDevice, const B: usize, const I: usize = NINODE> {
    cache: &'cache InodeCache<'device, D, B, I>,
    index: usize,
    inum: u32,
    live: bool,
}

impl<'cache, 'device, D: BlockDevice, const B: usize, const I: usize>
    InodeRef<'cache, 'device, D, B, I>
{
    pub fn inum(&self) -> u32 {
        self.inum
    }

    pub fn idup(&self) -> Result<Self, FsError> {
        self.cache.iget(self.inum)
    }

    pub fn lock(&self) -> Result<LockedInode<'_, 'device, D, B, I>, FsError> {
        let mut inode = self.cache.entries[self.index].inode.lock();
        if !inode.valid {
            let block = self.cache.buffers.bread(inode_block(self.inum) as u64)?;
            let range = inode_byte_range(self.inum)?;
            let disk = DiskInode::decode_from(&block.data()[range]).ok_or(FsError::CorruptImage)?;
            if disk.type_ == T_NONE {
                return Err(FsError::InodeNotAllocated);
            }
            inode.load(disk);
        }
        Ok(LockedInode {
            inode,
            cache: self.cache,
            inum: self.inum,
        })
    }

    /// Explicit `iput` counterpart. Destruction only changes cache metadata.
    pub fn iput(self) {
        drop(self);
    }
}

impl<D: BlockDevice, const B: usize, const I: usize> Drop for InodeRef<'_, '_, D, B, I> {
    fn drop(&mut self) {
        if self.live {
            self.cache.release(self.index, self.inum);
            self.live = false;
        }
    }
}

pub struct LockedInode<'entry, 'device, D: BlockDevice, const B: usize, const I: usize = NINODE> {
    inode: MutexGuard<'entry, Inode>,
    cache: &'entry InodeCache<'device, D, B, I>,
    inum: u32,
}

impl<D: BlockDevice, const B: usize, const I: usize> LockedInode<'_, '_, D, B, I> {
    pub fn inum(&self) -> u32 {
        self.inum
    }

    /// `iupdate`: write the locked in-memory inode to its disk slot.
    pub fn update(&mut self) -> Result<(), FsError> {
        let mut block = self.cache.buffers.bread(inode_block(self.inum) as u64)?;
        let range = inode_byte_range(self.inum)?;
        if !self
            .inode
            .to_disk()
            .encode_into(&mut block.data_mut()[range])
        {
            return Err(FsError::CorruptImage);
        }
        block.write()?;
        Ok(())
    }

    /// Resolve a file-relative block without allocating storage.
    pub fn bmap_readonly(&self, logical_bn: u32) -> Result<Option<u32>, FsError> {
        let logical_bn = usize::try_from(logical_bn).map_err(|_| FsError::OutOfRange)?;
        if logical_bn >= MAXFILE {
            return Err(FsError::FileTooLarge);
        }

        if logical_bn < NDIRECT {
            return self.validate_data_address(self.addrs[logical_bn]);
        }

        let indirect_bn = self.addrs[NDIRECT];
        if indirect_bn == 0 {
            return Ok(None);
        }
        self.validate_block_number(indirect_bn)?;

        let indirect_index = logical_bn - NDIRECT;
        if indirect_index >= NINDIRECT {
            return Err(FsError::FileTooLarge);
        }
        let byte_offset = indirect_index
            .checked_mul(core::mem::size_of::<u32>())
            .ok_or(FsError::OutOfRange)?;
        let byte_end = byte_offset
            .checked_add(core::mem::size_of::<u32>())
            .ok_or(FsError::OutOfRange)?;
        let block = self.cache.buffers.bread(indirect_bn as u64)?;
        let bytes: [u8; 4] = block.data()[byte_offset..byte_end]
            .try_into()
            .map_err(|_| FsError::CorruptImage)?;
        self.validate_data_address(u32::from_le_bytes(bytes))
    }

    /// Read file or directory bytes starting at `offset`.
    pub fn read_at(&self, dst: &mut [u8], offset: u32) -> Result<usize, FsError> {
        match self.type_ {
            T_FILE | T_DIR => {}
            T_DEV => return Err(FsError::Unsupported),
            _ => return Err(FsError::InvalidInodeType),
        }

        let max_size = MAXFILE.checked_mul(BLOCK_SIZE).ok_or(FsError::OutOfRange)?;
        let file_size = usize::try_from(self.size).map_err(|_| FsError::OutOfRange)?;
        if file_size > max_size {
            return Err(FsError::CorruptImage);
        }

        let offset = usize::try_from(offset).map_err(|_| FsError::OutOfRange)?;
        let requested_end = offset.checked_add(dst.len()).ok_or(FsError::OutOfRange)?;
        if dst.is_empty() || offset >= file_size {
            return Ok(0);
        }
        let end = requested_end.min(file_size);
        let read_len = end.checked_sub(offset).ok_or(FsError::OutOfRange)?;
        let mut copied = 0usize;

        while copied < read_len {
            let position = offset.checked_add(copied).ok_or(FsError::OutOfRange)?;
            let logical_bn =
                u32::try_from(position / BLOCK_SIZE).map_err(|_| FsError::OutOfRange)?;
            let block_offset = position % BLOCK_SIZE;
            let chunk_len = (BLOCK_SIZE - block_offset).min(read_len - copied);

            match self.bmap_readonly(logical_bn)? {
                Some(block_no) => {
                    let block = self.cache.buffers.bread(block_no as u64)?;
                    dst[copied..copied + chunk_len]
                        .copy_from_slice(&block.data()[block_offset..block_offset + chunk_len]);
                }
                None => dst[copied..copied + chunk_len].fill(0),
            }
            copied = copied.checked_add(chunk_len).ok_or(FsError::OutOfRange)?;
        }

        Ok(copied)
    }

    fn validate_data_address(&self, block_no: u32) -> Result<Option<u32>, FsError> {
        if block_no == 0 {
            return Ok(None);
        }
        self.validate_block_number(block_no)?;
        Ok(Some(block_no))
    }

    fn validate_block_number(&self, block_no: u32) -> Result<(), FsError> {
        if block_no >= self.cache.sb.size {
            return Err(FsError::CorruptImage);
        }
        Ok(())
    }
}

impl<D: BlockDevice, const B: usize, const I: usize> Deref for LockedInode<'_, '_, D, B, I> {
    type Target = Inode;
    fn deref(&self) -> &Self::Target {
        &self.inode
    }
}

impl<D: BlockDevice, const B: usize, const I: usize> DerefMut for LockedInode<'_, '_, D, B, I> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inode
    }
}

fn inode_byte_range(inum: u32) -> Result<core::ops::Range<usize>, FsError> {
    let start = inode_index_in_block(inum)
        .checked_mul(DiskInode::ENCODED_SIZE)
        .ok_or(FsError::CorruptImage)?;
    let end = start
        .checked_add(DiskInode::ENCODED_SIZE)
        .ok_or(FsError::CorruptImage)?;
    Ok(start..end)
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::fs::{
        block::{BLOCK_SIZE, BlockError},
        layout::{T_DIR, T_FILE},
    };

    const BLOCKS: usize = 32;
    const BUFFERS: usize = 4;
    static READS: AtomicUsize = AtomicUsize::new(0);

    struct TestDevice {
        blocks: Mutex<[[u8; BLOCK_SIZE]; BLOCKS]>,
        writes: AtomicUsize,
    }

    impl TestDevice {
        fn new() -> Self {
            Self {
                blocks: Mutex::new([[0; BLOCK_SIZE]; BLOCKS]),
                writes: AtomicUsize::new(0),
            }
        }

        fn put_inode(&self, inum: u32, inode: DiskInode) {
            let mut blocks = self.blocks.lock();
            let range = inode_byte_range(inum).expect("inode range");
            assert!(inode.encode_into(&mut blocks[inode_block(inum) as usize][range]));
        }

        fn put_block(&self, block_no: u32, data: &[u8]) {
            assert!(data.len() <= BLOCK_SIZE);
            let mut blocks = self.blocks.lock();
            let block = &mut blocks[block_no as usize];
            block.fill(0);
            block[..data.len()].copy_from_slice(data);
        }
    }

    impl BlockDevice for TestDevice {
        fn num_blocks(&self) -> u64 {
            BLOCKS as u64
        }

        fn read_block(&self, block_no: u64, dst: &mut [u8; BLOCK_SIZE]) -> Result<(), BlockError> {
            READS.fetch_add(1, Ordering::Relaxed);
            let blocks = self.blocks.lock();
            dst.copy_from_slice(
                blocks
                    .get(block_no as usize)
                    .ok_or(BlockError::OutOfRange)?,
            );
            Ok(())
        }

        fn write_block(&self, block_no: u64, src: &[u8; BLOCK_SIZE]) -> Result<(), BlockError> {
            self.writes.fetch_add(1, Ordering::Relaxed);
            let mut blocks = self.blocks.lock();
            blocks
                .get_mut(block_no as usize)
                .ok_or(BlockError::OutOfRange)?
                .copy_from_slice(src);
            Ok(())
        }
    }

    fn superblock() -> SuperBlock {
        SuperBlock {
            size: BLOCKS as u32,
            nblocks: 24,
            ninodes: 8,
            nlog: 0,
        }
    }

    fn allocated_inode(type_: u16) -> DiskInode {
        DiskInode {
            type_,
            major: 0,
            minor: 0,
            nlink: 1,
            size: 123,
            addrs: core::array::from_fn(|index| index as u32 + 20),
        }
    }

    fn file_inode(size: usize, addrs: [u32; NDIRECT + 3]) -> DiskInode {
        DiskInode {
            type_: T_FILE,
            major: 0,
            minor: 0,
            nlink: 1,
            size: u32::try_from(size).expect("file size"),
            addrs,
        }
    }

    fn disk_inode(buffers: &BufferCache<TestDevice, BUFFERS>, inum: u32) -> DiskInode {
        let block = buffers
            .bread(inode_block(inum) as u64)
            .expect("read inode block");
        let range = inode_byte_range(inum).expect("inode range");
        DiskInode::decode_from(&block.data()[range]).expect("disk inode")
    }

    #[test_case]
    fn iget_is_lazy_and_idup_shares_the_cache_entry() {
        READS.store(0, Ordering::Relaxed);
        let device = TestDevice::new();
        let buffers = BufferCache::<_, BUFFERS>::new(device);
        let cache = InodeCache::<_, BUFFERS, 2>::new(&buffers, superblock());

        let first = cache.iget(1).expect("iget");
        let second = first.idup().expect("idup");
        assert_eq!(first.index, second.index);
        assert_eq!(cache.refcnt(1), 2);
        assert_eq!(READS.load(Ordering::Relaxed), 0);

        drop(second);
        assert_eq!(cache.refcnt(1), 1);
        first.iput();
        assert_eq!(cache.refcnt(1), 0);
    }

    #[test_case]
    fn lock_lazy_loads_once_and_update_persists_fields() {
        READS.store(0, Ordering::Relaxed);
        let device = TestDevice::new();
        device.put_inode(1, allocated_inode(T_FILE));
        let buffers = BufferCache::<_, BUFFERS>::new(device);
        let cache = InodeCache::<_, BUFFERS, 2>::new(&buffers, superblock());
        let inode_ref = cache.iget(1).expect("iget");

        {
            let mut inode = inode_ref.lock().expect("lock");
            assert!(inode.valid);
            assert_eq!(inode.type_, T_FILE);
            assert_eq!(inode.size, 123);
            inode.size = 456;
            inode.nlink = 2;
            inode.addrs[0] = 99;
            inode.update().expect("iupdate");
        }
        assert_eq!(READS.load(Ordering::Relaxed), 1);

        drop(inode_ref.lock().expect("second lock"));
        assert_eq!(READS.load(Ordering::Relaxed), 1);
        let disk = disk_inode(&buffers, 1);
        assert_eq!(disk.size, 456);
        assert_eq!(disk.nlink, 2);
        assert_eq!(disk.addrs[0], 99);
    }

    #[test_case]
    fn zero_ref_entry_is_reused_and_full_cache_returns_error() {
        let device = TestDevice::new();
        let buffers = BufferCache::<_, BUFFERS>::new(device);
        let cache = InodeCache::<_, BUFFERS, 2>::new(&buffers, superblock());

        let first = cache.iget(1).expect("first");
        let first_index = first.index;
        let second = cache.iget(2).expect("second");
        assert!(matches!(cache.iget(3), Err(FsError::NoFreeInode)));
        drop(first);

        let third = cache.iget(3).expect("reused entry");
        assert_eq!(third.index, first_index);
        assert_eq!(*cache.entries[third.index].inode.lock(), Inode::EMPTY);
        drop((second, third));
    }

    #[test_case]
    fn ialloc_updates_the_disk_and_returns_an_unlocked_lazy_reference() {
        let device = TestDevice::new();
        device.put_inode(1, allocated_inode(T_DIR));
        let buffers = BufferCache::<_, BUFFERS>::new(device);
        let cache = InodeCache::<_, BUFFERS, 2>::new(&buffers, superblock());

        let allocated = cache.ialloc(T_FILE).expect("ialloc");
        assert_eq!(allocated.inum(), 2);
        assert_eq!(disk_inode(&buffers, 2).type_, T_FILE);
        assert_eq!(disk_inode(&buffers, 2).nlink, 0);
        assert!(!cache.entries[allocated.index].inode.lock().valid);

        let inode = allocated.lock().expect("lock allocated inode");
        assert_eq!(inode.type_, T_FILE);
        assert_eq!(inode.nlink, 0);
    }

    #[test_case]
    fn invalid_out_of_range_and_unallocated_inodes_are_errors() {
        let device = TestDevice::new();
        let buffers = BufferCache::<_, BUFFERS>::new(device);
        let cache = InodeCache::<_, BUFFERS, 2>::new(&buffers, superblock());

        assert!(matches!(cache.iget(0), Err(FsError::InvalidInode)));
        assert!(matches!(cache.iget(8), Err(FsError::InodeOutOfRange)));
        let unallocated = cache.iget(1).expect("cache unallocated inode");
        assert!(matches!(
            unallocated.lock(),
            Err(FsError::InodeNotAllocated)
        ));
        assert!(matches!(
            cache.ialloc(T_NONE),
            Err(FsError::InvalidInodeType)
        ));
    }

    #[test_case]
    fn read_at_handles_offsets_block_boundaries_and_eof() {
        let device = TestDevice::new();
        let first = core::array::from_fn::<_, BLOCK_SIZE, _>(|index| (index % 251) as u8);
        let second = [0xa5; BLOCK_SIZE];
        device.put_block(10, &first);
        device.put_block(11, &second);
        let mut addrs = [0; NDIRECT + 3];
        addrs[0] = 10;
        addrs[1] = 11;
        device.put_inode(1, file_inode(BLOCK_SIZE + 16, addrs));

        let buffers = BufferCache::<_, BUFFERS>::new(device);
        let cache = InodeCache::<_, BUFFERS, 2>::new(&buffers, superblock());
        let inode_ref = cache.iget(1).expect("iget");
        let inode = inode_ref.lock().expect("lock");

        let mut start = [0; 8];
        assert_eq!(inode.read_at(&mut start, 0), Ok(8));
        assert_eq!(start, first[..8]);

        let mut middle = [0; 7];
        assert_eq!(inode.read_at(&mut middle, 37), Ok(7));
        assert_eq!(middle, first[37..44]);

        let mut crossing = [0; 8];
        assert_eq!(inode.read_at(&mut crossing, (BLOCK_SIZE - 4) as u32), Ok(8));
        assert_eq!(&crossing[..4], &first[BLOCK_SIZE - 4..]);
        assert_eq!(&crossing[4..], &[0xa5; 4]);

        let mut beyond_eof = [0xcc; 20];
        assert_eq!(
            inode.read_at(&mut beyond_eof, (BLOCK_SIZE + 10) as u32),
            Ok(6)
        );
        assert_eq!(&beyond_eof[..6], &[0xa5; 6]);
        assert_eq!(&beyond_eof[6..], &[0xcc; 14]);
        assert_eq!(
            inode.read_at(&mut beyond_eof, (BLOCK_SIZE + 16) as u32),
            Ok(0)
        );
        assert_eq!(
            inode.read_at(&mut beyond_eof, (BLOCK_SIZE + 17) as u32),
            Ok(0)
        );
        assert_eq!(inode.read_at(&mut [], 0), Ok(0));
    }

    #[test_case]
    fn read_at_zero_fills_holes() {
        let device = TestDevice::new();
        device.put_inode(1, file_inode(24, [0; NDIRECT + 3]));
        let buffers = BufferCache::<_, BUFFERS>::new(device);
        let cache = InodeCache::<_, BUFFERS, 2>::new(&buffers, superblock());
        let inode_ref = cache.iget(1).expect("iget");
        let inode = inode_ref.lock().expect("lock");
        let mut bytes = [0xff; 24];

        assert_eq!(inode.read_at(&mut bytes, 0), Ok(24));
        assert_eq!(bytes, [0; 24]);
        assert_eq!(inode.bmap_readonly(0), Ok(None));
        assert_eq!(inode.bmap_readonly(NDIRECT as u32), Ok(None));
    }

    #[test_case]
    fn read_at_follows_indirect_blocks_across_the_direct_boundary() {
        let device = TestDevice::new();
        device.put_block(10, &[1; BLOCK_SIZE]);
        device.put_block(12, &[2; BLOCK_SIZE]);
        let mut indirect = [0; BLOCK_SIZE];
        indirect[..4].copy_from_slice(&12u32.to_le_bytes());
        device.put_block(11, &indirect);
        let mut addrs = [0; NDIRECT + 3];
        addrs[NDIRECT - 1] = 10;
        addrs[NDIRECT] = 11;
        device.put_inode(1, file_inode((NDIRECT + 1) * BLOCK_SIZE, addrs));

        let buffers = BufferCache::<_, BUFFERS>::new(device);
        let cache = InodeCache::<_, BUFFERS, 2>::new(&buffers, superblock());
        let inode_ref = cache.iget(1).expect("iget");
        let inode = inode_ref.lock().expect("lock");
        let mut bytes = [0; 8];

        assert_eq!(inode.bmap_readonly(NDIRECT as u32), Ok(Some(12)));
        assert_eq!(
            inode.read_at(&mut bytes, (NDIRECT * BLOCK_SIZE - 4) as u32),
            Ok(8)
        );
        assert_eq!(&bytes[..4], &[1; 4]);
        assert_eq!(&bytes[4..], &[2; 4]);
    }

    #[test_case]
    fn read_path_rejects_out_of_range_blocks_and_sizes() {
        let device = TestDevice::new();
        let mut indirect = [0; BLOCK_SIZE];
        indirect[..4].copy_from_slice(&(BLOCKS as u32).to_le_bytes());
        device.put_block(10, &indirect);
        device.put_inode(1, file_inode(1, [0; NDIRECT + 3]));
        let buffers = BufferCache::<_, BUFFERS>::new(device);
        let cache = InodeCache::<_, BUFFERS, 2>::new(&buffers, superblock());
        let inode_ref = cache.iget(1).expect("iget");
        let mut inode = inode_ref.lock().expect("lock");

        assert_eq!(
            inode.bmap_readonly(MAXFILE as u32),
            Err(FsError::FileTooLarge)
        );
        inode.addrs[0] = BLOCKS as u32;
        assert_eq!(inode.bmap_readonly(0), Err(FsError::CorruptImage));
        inode.addrs[0] = 0;
        inode.addrs[NDIRECT] = BLOCKS as u32;
        assert_eq!(
            inode.bmap_readonly(NDIRECT as u32),
            Err(FsError::CorruptImage)
        );
        inode.addrs[NDIRECT] = 10;
        assert_eq!(
            inode.bmap_readonly(NDIRECT as u32),
            Err(FsError::CorruptImage)
        );

        inode.size = (MAXFILE * BLOCK_SIZE + 1) as u32;
        assert_eq!(inode.read_at(&mut [0; 1], 0), Err(FsError::CorruptImage));
    }

    #[test_case]
    fn read_at_supports_directories_but_not_devices() {
        let device = TestDevice::new();
        device.put_block(10, &[7]);
        let mut addrs = [0; NDIRECT + 3];
        addrs[0] = 10;
        let mut directory = file_inode(1, addrs);
        directory.type_ = T_DIR;
        device.put_inode(1, directory);
        let mut device_inode = file_inode(0, [0; NDIRECT + 3]);
        device_inode.type_ = T_DEV;
        device.put_inode(2, device_inode);
        let buffers = BufferCache::<_, BUFFERS>::new(device);
        let cache = InodeCache::<_, BUFFERS, 2>::new(&buffers, superblock());
        let directory_ref = cache.iget(1).expect("iget directory");
        let directory = directory_ref.lock().expect("lock directory");
        let mut byte = [0];
        assert_eq!(directory.read_at(&mut byte, 0), Ok(1));
        assert_eq!(byte, [7]);
        drop(directory);

        let device_ref = cache.iget(2).expect("iget device");
        let device = device_ref.lock().expect("lock device");
        assert_eq!(device.read_at(&mut byte, 0), Err(FsError::Unsupported));
    }
}
