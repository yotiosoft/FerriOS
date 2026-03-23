/// パニックハンドラ
#[cfg(test)]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use super::init::*;
    test_panic_handler(info)
}

/// CPU を停止
pub fn hlt_loop() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}

extern crate alloc;
/// alloc エラーハンドラ
#[alloc_error_handler]
fn alloc_error_handler(layout: alloc::alloc::Layout) -> ! {
    panic!("allocation error: {:?}", layout)
}
