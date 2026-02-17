use core::fmt;
use spin::Mutex;
use lazy_static::lazy_static;

pub mod serial;
pub mod vga_buffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleMode {
    Serial,
    Vga,
    Both,
}

/// コンソール構造体
struct Console {
    mode: ConsoleMode,
}

impl Console {
    const fn new() -> Self {
        Console {
            mode: ConsoleMode::Both,
        }
    }

    fn detect_serial() -> bool {
        use x86_64::instructions::port::Port;

        unsafe {
            // Line Status Register
            let mut port = Port::<u8>::new(0x3FD);
            let status = port.read();
            // bit-5 と bit-6 が立っていればシリアルポートが存在する
            (status & 0x60) != 0
        }
    }

    pub fn mode_selector(&self) -> ConsoleMode {
        if Self::detect_serial() {
            ConsoleMode::Both
        }
        else {
            ConsoleMode::Vga
        }
    }
}

lazy_static! {
    static ref CONSOLE: Mutex<Console> = Mutex::new(Console::new());
}

pub fn _print(args: fmt::Arguments) {
    use x86_64::instructions::interrupts;

    // ロック中の割り込みを防止
    interrupts::without_interrupts(|| {
        let console = CONSOLE.lock();
        let mode = console.mode_selector();

        match mode {
            ConsoleMode::Serial => {
                serial::_print(args);
            },
            ConsoleMode::Vga => {
                vga_buffer::_print(args);
            },
            ConsoleMode::Both => {
                serial::_print(args);
                vga_buffer::_print(args);
            }
        }
    });
}

/// シリアルポートに文字列を書き込む
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::console::_print(format_args!($($arg)*))
    };
}

/// シリアルポートに文字列を書き込み、改行する
#[macro_export]
macro_rules! println {
    () => ($crate::console::_print("\n"));
    ($fmt:expr) => ($crate::print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::print!(concat!($fmt, "\n"), $($arg)*));
}

