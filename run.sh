#!/bin/bash
set -e

ARGS="$@"

./build.sh
qemu-system-x86_64 -drive format=raw,file=target/bios.img $ARGS
