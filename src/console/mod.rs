use core::fmt;
use spin::Mutex;
use lazy_static::lazy_static;

use crate::libbackend::test;

pub mod serial;
pub mod vga_buffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleMode {
    Serial,
    Vga,
    Both,
}

/// コンソール構造体
#[derive(Debug)]
pub struct Console {
    mode: ConsoleMode,
}

impl Console {
    const fn new() -> Self {
        Console {
            mode: ConsoleMode::Both,
        }
    }

    pub fn set(&mut self, mode: ConsoleMode) {
        self.mode = mode;
    }

    pub fn get(&self) -> ConsoleMode {
        self.mode
    }

    fn is_serial_avaiable() -> bool {
        use x86_64::instructions::port::Port;

        unsafe {
            // Line Status Register
            let mut port = Port::<u8>::new(0x3FD);
            let status = port.read();
            // bit-5 と bit-6 が立っていればシリアルポートが存在する
            (status & 0x60) != 0
        }
    }

    pub fn update_mode(&mut self) {
        self.mode = if Self::is_serial_avaiable() {
            ConsoleMode::Both
        }
        else {
            ConsoleMode::Vga
        };
    }
}

lazy_static! {
    pub static ref CONSOLE: Mutex<Console> = Mutex::new(Console::new());
}

pub fn init() {
    let mut console = CONSOLE.lock();
    console.update_mode();
}

pub fn _print(args: fmt::Arguments) {
    use x86_64::instructions::interrupts;

    // ロック中の割り込みを防止
    interrupts::without_interrupts(|| {
        let console = CONSOLE.lock();

        match console.mode {
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

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::console::_print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! println {
    () => ($crate::console::_print("\n"));
    ($fmt:expr) => ($crate::print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::print!(concat!($fmt, "\n"), $($arg)*));
}
