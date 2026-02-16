use uart_16550::SerialPort;
use spin::Mutex;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref SERIAL1: Mutex<SerialPort> = {
        let mut serial_port = unsafe {
            SerialPort::new(0x3F8)          // 最初のシリアルポートの標準の I/O ポート番号
        };
        serial_port.init();
        Mutex::new(serial_port)
    };
}

#[doc(hidden)]
pub fn _print(args: ::core::fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    // ロック中の割り込みを防止
    interrupts::without_interrupts(|| {
        SERIAL1.lock().write_fmt(args).expect("Printing to serial failed");
    });
}

/// シリアルポートに文字列を書き込む
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::console::serial::_print(format_args!($($arg)*));
    };
}

/// シリアルポートに文字列を書き込み、改行する
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($fmt:expr) => ($crate::serial_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::serial_print!(concat!($fmt, "\n"), $($arg)*));
}
