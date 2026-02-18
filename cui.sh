cargo build
cargo bootimage
qemu-system-x86_64 -nographic -serial mon:stdio -drive format=raw,file=target/x86_64-ferrios/debug/bootimage-ferrios.bin

