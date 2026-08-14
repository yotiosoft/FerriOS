use core::array;

use spin::{Mutex, MutexGuard};

use super::block::{self, BlockDevice, BlockError, BLOCK_SIZE};

pub const NBUF: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferError {
    Block(BlockError),
    NoFreeBuffer,
    BufferBusy,
    InvalidBufferState,
    AlreadyInitialized,
}

impl From<BlockError> for BufferError {
    fn from(error: BlockError) -> Self {
        Self::Block(error)
    }
}

/// キャッシュされたエントリのメタデータ
#[derive(Clone, Copy)]
struct EntryMeta {
    block_no: Option<u64>,
    valid: bool,
    dirty: bool,
    busy: bool,
}

impl EntryMeta {
    const EMPTY: Self = Self {
        block_no: None,
        valid: false,
        dirty: false,
        busy: false,
    };
}

/// キャッシュエントリのバイナリ
struct CacheEntry {
    data: Mutex<[u8; BLOCK_SIZE]>,
}

/// キャッシュ本体
/// メタデータ EntryMeta と、エントリ CacheEntry を持つ
pub struct BufferCache<D, const N: usize = NBUF> {
    device: D,
    metadata: Mutex<[EntryMeta; N]>,
    entries: [CacheEntry; N],
}

impl<D: BlockDevice, const N: usize> BufferCache<D, N> {
    /// 新しいキャッシュエントリを作成
    pub fn new(device: D) -> Self {
        Self {
            device,
            metadata: Mutex::new([EntryMeta::EMPTY; N]),
            entries: array::from_fn(|_| CacheEntry {
                data: Mutex::new([0; BLOCK_SIZE]),
            }),
        }
    }

    pub fn device_blocks(&self) -> u64 {
        self.device.num_blocks()
    }

    /// バッファ読み込み
    /// block_no を指定し、バイナリを保持する BufferGuard を返す
    pub fn bread(&self, block_no: u64) -> Result<BufferGuard<'_, D, N>, BufferError> {
        if block_no >= self.device.num_blocks() {
            return Err(BufferError::Block(BlockError::OutOfRange));
        }

        let (index, needs_read) = {
            let mut metadata = self.metadata.lock();

            if let Some(index) = metadata
                .iter()
                .position(|entry| entry.block_no == Some(block_no))
            {
                if metadata[index].busy {
                    return Err(BufferError::BufferBusy);
                }
                if !metadata[index].valid {
                    return Err(BufferError::InvalidBufferState);
                }
                metadata[index].busy = true;
                (index, false)
            } else {
                let index = metadata
                    .iter()
                    .position(|entry| !entry.busy && !entry.dirty)
                    .ok_or(BufferError::NoFreeBuffer)?;
                metadata[index] = EntryMeta {
                    block_no: Some(block_no),
                    valid: false,
                    dirty: false,
                    busy: true,
                };
                (index, true)
            }
        };

        let mut data = self.entries[index].data.lock();
        if needs_read {
            if let Err(error) = self.device.read_block(block_no, &mut data) {
                let mut metadata = self.metadata.lock();
                metadata[index] = EntryMeta::EMPTY;
                return Err(error.into());
            }
            self.metadata.lock()[index].valid = true;
        }

        Ok(BufferGuard {
            cache: self,
            index,
            block_no,
            data: Some(data),
        })
    }

    /// バッファに dirty をマーキングする
    fn mark_dirty(&self, index: usize, block_no: u64) -> Result<(), BufferError> {
        let mut metadata = self.metadata.lock();
        let entry = metadata
            .get_mut(index)
            .ok_or(BufferError::InvalidBufferState)?;
        if !entry.busy || !entry.valid || entry.block_no != Some(block_no) {
            return Err(BufferError::InvalidBufferState);
        }
        entry.dirty = true;
        Ok(())
    }

    /// バッファ書き込み
    /// data バイナリを index で指定したデバイスの block_no に書き込む
    fn write(
        &self,
        index: usize,
        block_no: u64,
        data: &[u8; BLOCK_SIZE],
    ) -> Result<(), BufferError> {
        self.mark_dirty(index, block_no)?;
        self.device.write_block(block_no, data)?;

        let mut metadata = self.metadata.lock();
        let entry = &mut metadata[index];
        if !entry.busy || entry.block_no != Some(block_no) {
            return Err(BufferError::InvalidBufferState);
        }
        entry.dirty = false;
        Ok(())
    }

    /// バッファキャッシュの busy 状態を解除
    fn release(&self, index: usize, block_no: u64) {
        let mut metadata = self.metadata.lock();
        let entry = &mut metadata[index];
        if entry.busy && entry.block_no == Some(block_no) {
            entry.busy = false;
        }
    }

    /// バッファキャッシュは dirty か？
    #[cfg(test)]
    fn is_dirty(&self, block_no: u64) -> bool {
        self.metadata
            .lock()
            .iter()
            .any(|entry| entry.block_no == Some(block_no) && entry.dirty)
    }
}

/// read したバッファの guard
/// コピーされたバッファを保持する
pub struct BufferGuard<'a, D: BlockDevice, const N: usize = NBUF> {
    cache: &'a BufferCache<D, N>,
    index: usize,
    block_no: u64,
    data: Option<MutexGuard<'a, [u8; BLOCK_SIZE]>>,
}

impl<D: BlockDevice, const N: usize> BufferGuard<'_, D, N> {
    pub fn block_no(&self) -> u64 {
        self.block_no
    }

    pub fn data(&self) -> &[u8; BLOCK_SIZE] {
        self.data.as_ref().expect("buffer guard data")
    }

    pub fn data_mut(&mut self) -> &mut [u8; BLOCK_SIZE] {
        self.cache
            .mark_dirty(self.index, self.block_no)
            .expect("live buffer guard");
        self.data.as_mut().expect("buffer guard data")
    }

    pub fn mark_dirty(&mut self) -> Result<(), BufferError> {
        self.cache.mark_dirty(self.index, self.block_no)
    }

    pub fn write(&mut self) -> Result<(), BufferError> {
        self.cache.write(
            self.index,
            self.block_no,
            self.data.as_ref().ok_or(BufferError::InvalidBufferState)?,
        )
    }

    pub fn release(self) {}
}

impl<D: BlockDevice, const N: usize> Drop for BufferGuard<'_, D, N> {
    fn drop(&mut self) {
        self.data.take();
        self.cache.release(self.index, self.block_no);
    }
}

#[doc(hidden)]
pub struct RootBlockDevice;

impl BlockDevice for RootBlockDevice {
    fn num_blocks(&self) -> u64 {
        block::root_num_blocks().unwrap_or(0)
    }

    fn read_block(&self, block_no: u64, dst: &mut [u8; BLOCK_SIZE]) -> Result<(), BlockError> {
        block::read_root_block(block_no, dst)
    }

    fn write_block(&self, block_no: u64, src: &[u8; BLOCK_SIZE]) -> Result<(), BlockError> {
        block::write_root_block(block_no, src)
    }
}

lazy_static::lazy_static! {
    static ref ROOT_CACHE: BufferCache<RootBlockDevice> = BufferCache::new(RootBlockDevice);
}

static ROOT_CACHE_INITIALIZED: Mutex<bool> = Mutex::new(false);

pub fn init() -> Result<(), BufferError> {
    block::root_num_blocks()?;
    let mut initialized = ROOT_CACHE_INITIALIZED.lock();
    if *initialized {
        return Err(BufferError::AlreadyInitialized);
    }
    lazy_static::initialize(&ROOT_CACHE);
    *initialized = true;
    Ok(())
}

pub fn bread(block_no: u64) -> Result<BufferGuard<'static, RootBlockDevice>, BufferError> {
    if !*ROOT_CACHE_INITIALIZED.lock() {
        return Err(BufferError::Block(BlockError::NotInitialized));
    }
    ROOT_CACHE.bread(block_no)
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct TestDevice<const B: usize> {
        blocks: Mutex<[[u8; BLOCK_SIZE]; B]>,
        reads: AtomicUsize,
        writes: AtomicUsize,
        fail_read: Mutex<bool>,
        fail_write: Mutex<bool>,
    }

    impl<const B: usize> TestDevice<B> {
        fn new() -> Self {
            Self {
                blocks: Mutex::new([[0; BLOCK_SIZE]; B]),
                reads: AtomicUsize::new(0),
                writes: AtomicUsize::new(0),
                fail_read: Mutex::new(false),
                fail_write: Mutex::new(false),
            }
        }
    }

    impl<const B: usize> BlockDevice for TestDevice<B> {
        fn num_blocks(&self) -> u64 {
            B as u64
        }

        fn read_block(&self, block_no: u64, dst: &mut [u8; BLOCK_SIZE]) -> Result<(), BlockError> {
            if *self.fail_read.lock() {
                return Err(BlockError::NotInitialized);
            }
            let blocks = self.blocks.lock();
            let src = blocks
                .get(block_no as usize)
                .ok_or(BlockError::OutOfRange)?;
            dst.copy_from_slice(src);
            self.reads.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        fn write_block(&self, block_no: u64, src: &[u8; BLOCK_SIZE]) -> Result<(), BlockError> {
            if *self.fail_write.lock() {
                return Err(BlockError::NotInitialized);
            }
            let mut blocks = self.blocks.lock();
            let dst = blocks
                .get_mut(block_no as usize)
                .ok_or(BlockError::OutOfRange)?;
            dst.copy_from_slice(src);
            self.writes.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

    #[test_case]
    fn cache_miss_reads_and_cache_hit_does_not() {
        let device = TestDevice::<2>::new();
        device.blocks.lock()[1][0] = 0x5a;
        let cache = BufferCache::<_, 2>::new(device);

        assert_eq!(cache.bread(1).expect("first read").data()[0], 0x5a);
        assert_eq!(cache.device.reads.load(Ordering::Relaxed), 1);
        assert_eq!(cache.bread(1).expect("cached read").data()[0], 0x5a);
        assert_eq!(cache.device.reads.load(Ordering::Relaxed), 1);
    }

    #[test_case]
    fn busy_block_is_not_duplicated() {
        let cache = BufferCache::<_, 2>::new(TestDevice::<2>::new());
        let guard = cache.bread(0).expect("first holder");

        assert!(matches!(cache.bread(0), Err(BufferError::BufferBusy)));
        drop(guard);
        assert!(cache.bread(0).is_ok());
    }

    #[test_case]
    fn write_is_immediate_and_clears_dirty() {
        let cache = BufferCache::<_, 2>::new(TestDevice::<2>::new());
        let mut guard = cache.bread(0).expect("read block");
        guard.data_mut()[7] = 0xa5;
        assert!(cache.is_dirty(0));
        guard.write().expect("write block");
        assert!(!cache.is_dirty(0));
        drop(guard);

        assert_eq!(cache.device.blocks.lock()[0][7], 0xa5);
        assert_eq!(cache.device.writes.load(Ordering::Relaxed), 1);
    }

    #[test_case]
    fn clean_released_entry_can_be_evicted() {
        let cache = BufferCache::<_, 1>::new(TestDevice::<2>::new());
        drop(cache.bread(0).expect("read first block"));
        drop(cache.bread(1).expect("evict first block"));

        assert_eq!(cache.device.reads.load(Ordering::Relaxed), 2);
    }

    #[test_case]
    fn held_or_dirty_entry_is_not_evicted() {
        let cache = BufferCache::<_, 1>::new(TestDevice::<2>::new());
        let guard = cache.bread(0).expect("read first block");
        assert!(matches!(cache.bread(1), Err(BufferError::NoFreeBuffer)));
        drop(guard);

        let mut guard = cache.bread(0).expect("read cached block");
        guard.data_mut()[0] = 1;
        drop(guard);
        assert!(matches!(cache.bread(1), Err(BufferError::NoFreeBuffer)));
    }

    #[test_case]
    fn failed_read_does_not_leave_a_cache_hit() {
        let cache = BufferCache::<_, 1>::new(TestDevice::<1>::new());
        *cache.device.fail_read.lock() = true;
        assert!(matches!(
            cache.bread(0),
            Err(BufferError::Block(BlockError::NotInitialized))
        ));
        *cache.device.fail_read.lock() = false;

        assert!(cache.bread(0).is_ok());
        assert_eq!(cache.device.reads.load(Ordering::Relaxed), 1);
    }

    #[test_case]
    fn failed_write_preserves_dirty_state() {
        let cache = BufferCache::<_, 1>::new(TestDevice::<1>::new());
        let mut guard = cache.bread(0).expect("read block");
        guard.data_mut()[0] = 1;
        *cache.device.fail_write.lock() = true;

        assert!(guard.write().is_err());
        assert!(cache.is_dirty(0));
    }

    #[test_case]
    fn out_of_range_is_reported() {
        let cache = BufferCache::<_, 1>::new(TestDevice::<1>::new());
        assert!(matches!(
            cache.bread(1),
            Err(BufferError::Block(BlockError::OutOfRange))
        ));
    }
}
