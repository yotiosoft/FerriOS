use conquer_once::spin::OnceCell;
use crossbeam_queue::ArrayQueue;
use core::{ pin::Pin, task::{ Poll, Context } };
use futures_util::{ stream::{ Stream, StreamExt }, task::AtomicWaker };
use pc_keyboard::{ layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1 };
use crate::{ println, print };

static SCANCODE_QUEUE: OnceCell<ArrayQueue<u8>> = OnceCell::uninit();
static WAKER: AtomicWaker = AtomicWaker::new();

/// キーボードのコードをスキャンする
/// キーボード割り込みハンドラから呼び出される
pub(crate) fn add_scancode(scancode: u8) {
    if let Ok(queue) = SCANCODE_QUEUE.try_get() {
        if let Err(_) = queue.push(scancode) {
            // スキャンコードキューが満杯の場合
            println!("WARNING: scancode queue full; dropping keyboard input");
        }
        else {
            // 保存されている waker を起こす
            WAKER.wake();
        }
    }
    else {
        // スキャンコードキューが初期化されていない場合
        println!("WARNING: scancode queue uninitalized");
    }
}

/// スキャンコードストリーム
pub struct ScancodeStream {
    _private: (),       // モジュールの外部から構造体を構築できないようにする（＝ new() でしか構築できない）
}

impl ScancodeStream {
    pub fn new() -> Self {
        SCANCODE_QUEUE.try_init_once(|| ArrayQueue::new(100)).expect("ScancodeStream::new should only be called once");
        ScancodeStream {
            _private: ()
        }
    }
}

impl Stream for ScancodeStream {
    type Item = u8;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let queue = SCANCODE_QUEUE.try_get().expect("not initalized");

        // pop に成功したら ready を返す
        if let Some(scancode) = queue.pop() {
            return Poll::Ready(Some(scancode));
        }

        // 2回めの pop
        WAKER.register(&cx.waker());    // チェック後に push された全スキャンコードに対して wakeup が得られるよう保証する
        match queue.pop() {
            Some(scancode) => {
                WAKER.take();
                Poll::Ready(Some(scancode))
            }
            None => Poll::Pending,
        }
    }
}

/// 非同期キーボードタスク
pub async fn print_keypresses() {
    let mut scancodes = ScancodeStream::new();
    let mut keyboard = Keyboard::new(ScancodeSet1::new(), layouts::Us104Key, HandleControl::Ignore);

    // stream が None を返すまでループ
    while let Some(scancode) = scancodes.next().await {
        if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
            if let Some(key) = keyboard.process_keyevent(key_event) {
                match key {
                    DecodedKey::Unicode(character) => print!("{}", character),
                    DecodedKey::RawKey(key) => print!("{:?}", key),
                }
            }
        }
    }
}
