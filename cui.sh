cargo build
#cargo bootimage
#qemu-system-x86_64 -nographic -serial mon:stdio -drive format=raw,file=target/x86_64-ferrios/debug/bootimage-ferrios.bin
qemu-system-x86_64 -drive format=raw,file=target/x86_64-unknown-none/debug/build/ferrios-5b274071b2a96608/out/bios.img -serial stdio

