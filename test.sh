#!/bin/bash

find target/x86_64-unknown-none/release/build/ferrios-*/out/ -name "*.img" ! -name "bios.img" -delete 2>/dev/null || true

cargo test --no-run --release 2>&1
touch build.rs
cargo test --no-run --release 2>&1

FAILED=0

for img in target/x86_64-unknown-none/release/build/ferrios-*/out/*.img; do
    name=$(basename "$img" .img)
    [ "$name" = "bios" ] && continue

    echo "Running test: $name"
    timeout 60 qemu-system-x86_64 \
        -drive format=raw,file="$img" \
        -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
        -serial stdio \
        -display none \
        -no-reboot
    EXIT_CODE=$?

    if [ $EXIT_CODE -eq 33 ]; then
        echo "[ok] $name"
    elif [ $EXIT_CODE -eq 124 ]; then
        echo "[timeout] $name"
        FAILED=1
    else
        echo "[failed] $name (exit code: $EXIT_CODE)"
        FAILED=1
    fi
done

if [ $FAILED -eq 1 ]; then
    echo "Some tests failed!"
    exit 1
else
    echo "All tests passed!"
fi
