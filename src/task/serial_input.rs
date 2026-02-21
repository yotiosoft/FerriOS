use conquer_once::spin::OnceCell;
use crossbeam_queue::ArrayQueue;
use core::{ pin::Pin, task::{ Poll, Context } };
use futures_util::{ stream::Stream, task::AtomicWaker };
use futures_util::stream::StreamExt;
use crate::{ print, println };

static INPUT_QUEUE: OnceCell<ArrayQueue<u8>> = OnceCell::uninit();
static WAKER: AtomicWaker = AtomicWaker::new();

/// シリアル割り込みハンドラから呼び出される
pub(crate) fn add_byte(byte: u8) {
    if let Ok(queue) = INPUT_QUEUE.try_get() {
        if queue.push(byte).is_err() {
            panic!("serial_input queue is full");
        }
        else {
            WAKER.wake();
        }
    }
}

pub struct SerialInputStream {
    _private: (),
}

impl SerialInputStream {
    pub fn new() -> Self {
        INPUT_QUEUE.try_init_once(|| ArrayQueue::new(100)).expect("SerialInputStream::new should only be called once");
        SerialInputStream { _private: () }
    }
}

impl Stream for SerialInputStream {
    type Item = u8;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let queue = INPUT_QUEUE.try_get().expect("not initialized");

        if let Some(byte) = queue.pop() {
            return Poll::Ready(Some(byte));
        }

        WAKER.register(&cx.waker());
        match queue.pop() {
            Some(byte) => {
                WAKER.take();
                Poll::Ready(Some(byte))
            }
            None => Poll::Pending,
        }
    }
}

// シリアル入力タスク
pub async fn thread_serial_input() {
    let mut stream = SerialInputStream::new();

    while let Some(byte) = stream.next().await {
        match byte {
            b'\r' | b'\n' => println!(""),
            0x7F | 0x08 => print!("\x08 \x08"),
            0x20..=0x7E => print!("{}", byte as char),
            _ => {}
        }
    }
}
