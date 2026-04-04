# FerriOS Copilot Instructions

## Project Overview
FerriOS is a Rust-based hobby OS kernel inspired by xv6 and blog_os. It implements Unix-like features including threading, scheduling, memory management, and syscalls on x86_64 architecture.

## Architecture
- **Kernel Structure**: Monolithic kernel with modules in `kernel/src/` (interrupts, memory, allocator, task, thread, scheduler, syscall, exec)
- **Task Model**: Dual async/task system - cooperative async tasks via `task::Executor` and preemptive kernel threads via `thread::Thread` with scheduler
- **Memory**: Custom paging with fixed heap allocator (`allocator::FixedSizeBlockAllocator`) at `0xFFFF_8888_0000_0000`
- **Bootloader**: Uses `bootloader` crate with fixed virtual address mappings (kernel at `0xFFFF_8000_0000_0000`)

## Key Workflows
- **Build**: `cargo bootimage` creates BIOS image; `./build.sh` builds release and copies to `target/bios.img`
- **Run**: `./run.sh` (GUI) or `./run.sh -nographic -serial mon:stdio` (CUI) launches QEMU
- **Test**: `./test.sh` builds tests, runs each via QEMU with `isa-debug-exit` for automated exit
- **Debug**: Use `qemu-system-x86_64 -s -S` for GDB; breakpoints in `kernel/src/main.rs::kernel_main`

## Conventions
- **Async Tasks**: Use `task::Task::new(async move { ... })` and `executor.spawn()` for cooperative concurrency
- **Kernel Threads**: Create via `thread::kthread::create()` with function pointer; scheduler manages preemption
- **Memory Allocation**: `alloc::boxed::Box`, `alloc::vec::Vec` on heap; no_std with custom alloc
- **Synchronization**: `spin::Mutex` for locks, `crossbeam_queue::ArrayQueue` for task queues
- **Syscalls**: Implemented via `syscall` instruction; handler in `syscall::ksyscall` with naked asm
- **Error Handling**: Panic on critical errors; custom `libbackend` for test/init utilities
- **Code Style**: Snake_case modules, PascalCase types; extensive use of `lazy_static!` and `OnceCell`

## Examples
- Spawn async task: `executor.spawn(Task::new(async { println!("Hello"); }));`
- Create kernel thread: `kthread::create(user_function, stack_ptr, arg);`
- Allocate heap: `let v = vec![1, 2, 3];` (uses global allocator)
- Syscall dispatch: Match `rax` in `ksyscall::syscall()` for syscall number routing

Reference: `kernel/src/main.rs` for init flow, `kernel/src/task/executor.rs` for async impl, `kernel/src/thread/mod.rs` for thread model.</content>
<parameter name="filePath">/home/ytani/git/ferrios/.github/copilot-instructions.md