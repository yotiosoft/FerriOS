use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use lazy_static::lazy_static;
use pic8259::ChainedPics;
use spin;
use crate::{print, println};
use crate::gdt;
use crate::hlt_loop;
use crate::scheduler;

// まだヒープが存在しないため、IDT は静的変数として定義する
lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        unsafe {
            idt.double_fault.set_handler_fn(double_fault_handler).set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX)
        };
        idt[InterruptIndex::Timer.as_usize()].set_handler_fn(timer_interrupt_handler);
        idt[InterruptIndex::Keyboard.as_usize()].set_handler_fn(keyboard_interrupt_handler);
        idt[InterruptIndex::Serial.as_usize()].set_handler_fn(serial_interrupt_handler);
        idt.page_fault.set_handler_fn(page_fault_handler);

        idt
    };
}
pub fn init_idt() {
    IDT.load();
}

/// ブレークポイント例外ハンドラ
extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

/// ダブルフォルト例外ハンドラ
extern "x86-interrupt" fn double_fault_handler(stack_frame: InterruptStackFrame, _error_code: u64) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

/// ページフォルトハンドラ
extern "x86-interrupt" fn page_fault_handler(stack_frame: InterruptStackFrame, error_code: PageFaultErrorCode) {
    use x86_64::registers::control::Cr2;

    println!("EXCEPTION: PAGE FAULT");
    println!("Accessed Address: {:?}", Cr2::read());
    println!("Error Code: {:?}", error_code);
    println!("{:#?}", stack_frame);
    hlt_loop();
}

/// タイマ割り込みハンドラ
extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    unsafe {
        PICS.lock().notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }

    unsafe {
        if crate::scheduler::SCHEDULER_STARTED {
            scheduler::yield_from_context();

            PICS.lock().notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
        }
    }
}

/// キーボード割り込みハンドラ
extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    use x86_64::instructions::port::Port;
    
    let mut port = Port::new(0x60);
    let scancode: u8 = unsafe {
        port.read()
    };
    crate::task::keyboard::add_scancode(scancode);

    unsafe {
        PICS.lock().notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}

/// シリアル割り込みハンドラ
extern "x86-interrupt" fn serial_interrupt_handler(_stack_frame: InterruptStackFrame) {
    use x86_64::instructions::port::Port;

    let mut port = Port::new(0x3F8);
    let byte: u8 = unsafe {
        port.read()
    };

    // キーボードタスクに渡す
    crate::task::serial_input::add_byte(byte);

    unsafe {
        PICS.lock().notify_end_of_interrupt(InterruptIndex::Serial.as_u8());
    }
}

// 割り込み
pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;
// PIC の offset は 32 ~ 47 にマップ
pub static PICS: spin::Mutex<ChainedPics> = spin::Mutex::new(
    unsafe { 
        ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) 
    }
);

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Keyboard,
    Serial = PIC_1_OFFSET + 4,  // COM1, IRQ4
}
impl InterruptIndex {
    fn as_u8(self) -> u8 {
        self as u8
    }
    fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}

#[test_case]
fn test_breakpoint_exception() {
    // invoke a breakpoint exception
    x86_64::instructions::interrupts::int3();
}

#[test_case]
fn check_interrupt_indexes() {
    assert!(InterruptIndex::Timer.as_usize() == 32);
    assert!(InterruptIndex::Keyboard.as_usize() == 33);
}
