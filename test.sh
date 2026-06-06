#!/bin/bash
set -euo pipefail

BUILD_PROFILE=${FERRIOS_KERNEL_TEST_PROFILE:-release}
if [ "$BUILD_PROFILE" != "release" ] && [ "$BUILD_PROFILE" != "debug" ]; then
    echo "FERRIOS_KERNEL_TEST_PROFILE must be 'release' or 'debug'."
    exit 1
fi

PROFILE_FLAG=()
if [ "$BUILD_PROFILE" = "release" ]; then
    PROFILE_FLAG=(--release)
fi

find "target/$BUILD_PROFILE/build"/ferrios-runner-*/out/kernel-tests -name "*.img" -delete 2>/dev/null || true

echo "Building kernel tests in $BUILD_PROFILE profile"

FERRIOS_BUILD_KERNEL_TESTS=1 cargo test --no-run "${PROFILE_FLAG[@]}" 2>&1

mapfile -t IMAGES < <(find "target/$BUILD_PROFILE/build"/ferrios-runner-*/out/kernel-tests -name "*.img" 2>/dev/null | sort)

if [ "${#IMAGES[@]}" -eq 0 ]; then
    echo "No kernel test images were generated."
    exit 1
fi

FAILED=0
TIMEOUT_SECONDS=${FERRIOS_KERNEL_TEST_TIMEOUT:-300}
QEMU_ACCEL_ARGS=()
if [ "${FERRIOS_QEMU_ACCEL:-auto}" = "kvm" ]; then
    QEMU_ACCEL_ARGS=(-enable-kvm)
elif [ "${FERRIOS_QEMU_ACCEL:-auto}" = "tcg" ]; then
    QEMU_ACCEL_ARGS=(-accel tcg)
elif [ -r /dev/kvm ] && [ -w /dev/kvm ]; then
    QEMU_ACCEL_ARGS=(-enable-kvm)
else
    QEMU_ACCEL_ARGS=(-accel tcg)
fi

for img in "${IMAGES[@]}"; do
    name=$(basename "$img" .img)
    log="target/$BUILD_PROFILE/kernel-test-$name.log"
    rm -f "$log"
    touch "$log"

    echo "Running test: $name"
    echo "Image: $img"
    echo "QEMU acceleration: ${QEMU_ACCEL_ARGS[*]}"
    echo "Serial log: $log"
    set +e
    tail -n +1 -F "$log" &
    TAIL_PID=$!
    qemu-system-x86_64 \
        "${QEMU_ACCEL_ARGS[@]}" \
        -drive format=raw,file="$img" \
        -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
        -serial "file:$log" \
        -display none \
        -monitor none \
        -no-shutdown \
        -no-reboot &
    QEMU_PID=$!

    EXIT_CODE=124
    for _ in $(seq 1 "$TIMEOUT_SECONDS"); do
        if ! kill -0 "$QEMU_PID" 2>/dev/null; then
            wait "$QEMU_PID"
            EXIT_CODE=$?
            break
        fi

        if grep -q "All kernel tests passed" "$log"; then
            kill "$QEMU_PID" 2>/dev/null || true
            wait "$QEMU_PID" 2>/dev/null || true
            EXIT_CODE=33
            break
        fi

        if grep -q "\[failed\]" "$log"; then
            kill "$QEMU_PID" 2>/dev/null || true
            wait "$QEMU_PID" 2>/dev/null || true
            EXIT_CODE=1
            break
        fi

        sleep 1
    done

    if kill -0 "$QEMU_PID" 2>/dev/null; then
        kill "$QEMU_PID" 2>/dev/null || true
        wait "$QEMU_PID" 2>/dev/null || true
    fi
    kill "$TAIL_PID" 2>/dev/null || true
    wait "$TAIL_PID" 2>/dev/null || true
    set -e

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
