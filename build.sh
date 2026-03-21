#!/bin/bash
set -e
cargo build --release
IMG=$(find target/x86_64-unknown-none/release/build/ferrios-*/out/bios.img 2>/dev/null | head -1)
cp "$IMG" target/bios.img
echo "bios.img -> target/bios.img"
