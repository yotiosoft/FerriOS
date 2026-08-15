use core::{
    array,
    ops::{Deref, DerefMut},
};

use spin::{Mutex, MutexGuard};

use super::{
    bcache::BufferCache,
    block::BlockDevice,
    layout::{DiskInode, NDIRECT, SuperBlock, T_NONE, inode_block, inode_index_in_block},
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

/// Fixed-size xv6-style inode cache.
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

    /// xv6 `iget`: acquire a cache reference without reading the inode block.
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

    /// xv6 `ialloc`: allocate an on-disk inode and return it unlocked.
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

    /// xv6 `iupdate`: write the locked in-memory inode to its disk slot.
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

    const BLOCKS: usize = 16;
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
            nblocks: 10,
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
}
