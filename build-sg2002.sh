#!/usr/bin/env bash
# Cross-build akars for the SG2002 (AKA-00) board: RISC-V 64 musl, double-float.
#
# Why this script exists (see also .cargo/config.toml):
#   - The board runs a musl rootfs; its dynamic loader is
#     /lib/ld-musl-riscv64v0p7_xthead.so.1. The original C++ aka0 project links
#     the same TPU/OpenCV SDK with the musl toolchain, so we target
#     riscv64gc-unknown-linux-musl (NOT glibc, NOT bare-metal -none-elf).
#   - That target's std is not installed via rustup, so we build it from source
#     with -Zbuild-std (requires the rust-src component on a nightly toolchain).
#   - musl targets default to static-crt; we force dynamic (-crt-static) so the
#     binary uses the board's musl loader and the SDK's dynamic .so files.
#   - rustc emits the default musl loader name (ld-musl-riscv64.so.1), but the
#     board only has the xthead variant, so we override --dynamic-linker to match
#     what the original binaries use.
#   - The SDK's GNU ld (binutils 2.35) rejects modern RISC-V ISA attributes, so
#     we link with rust-lld; -B points gcc's collect2 at it.
#
# Usage:
#   ./build-sg2002.sh                 # release build
#   ./build-sg2002.sh --frames 1 ...  # extra args are forwarded to cargo
set -euo pipefail

# Loader path on the device. Must match what the board's rootfs provides and
# what the original aka0 binaries request.
DYN_LINKER="${DYN_LINKER:-/lib/ld-musl-riscv64v0p7_xthead.so.1}"

# --- SDK paths (override by exporting before calling) ---------------------
export TPU_SDK_PATH="${TPU_SDK_PATH:-/home/ajax/Proj/OS/sg2002/cvitek_tpu_sdk}"
export OPENCV_PATH="${OPENCV_PATH:-$TPU_SDK_PATH/opencv}"

# --- Cross toolchain (SG2002 / LicheeRV Nano host-tools, musl) -------------
TC_BIN="${TC_BIN:-/home/ajax/Proj/OS/sg2002/nano-linux/LicheeRV-Nano-Build/host-tools-master/gcc/riscv64-linux-musl-x86_64/bin}"
if [[ ! -x "$TC_BIN/riscv64-unknown-linux-musl-gcc" ]]; then
  echo "error: cross gcc not found at $TC_BIN" >&2
  echo "       set TC_BIN to the dir containing riscv64-unknown-linux-musl-gcc" >&2
  exit 1
fi

# --- rust-lld (modern linker; SDK ld is too old for new ISA attrs) ---------
LLD_DIR="$(rustc --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/gcc-ld"
if [[ ! -x "$LLD_DIR/ld.lld" ]]; then
  echo "error: rust-lld not found at $LLD_DIR/ld.lld" >&2
  echo "       ensure you are on a rustup-managed nightly toolchain" >&2
  exit 1
fi

export PATH="$TC_BIN:$PATH"

# Link flags for this target (overrides .cargo/config.toml rustflags, leaves the
# `linker` key there intact):
#   -fuse-ld=lld -B<dir> : use rust-lld via gcc's collect2
#   -crt-static off      : dynamic libc, so SDK .so + board musl loader are used
#   --dynamic-linker     : the board's actual musl loader name
export CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_RUSTFLAGS="\
-Clink-arg=-fuse-ld=lld \
-Clink-arg=-B$LLD_DIR \
-Ctarget-feature=-crt-static \
-Clink-arg=-Wl,--dynamic-linker=$DYN_LINKER"

# build.rs and the C++ bridge use these.
export CC="${CC:-riscv64-unknown-linux-musl-gcc}"
export CXX="${CXX:-riscv64-unknown-linux-musl-g++}"
export AR="${AR:-riscv64-unknown-linux-musl-ar}"

exec cargo build --release \
  --target riscv64gc-unknown-linux-musl \
  -Zbuild-std=std,panic_abort \
  "$@"
