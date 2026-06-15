# akars

Rust rewrite entry point for AKA-00 hardware control on SG2002.

Implemented scope:

- USB camera capture from `/dev/cvi-usb-camera0` using the StarryOS ioctl protocol.
- MJPEG decode and letterbox preprocessing through the SG2002 SDK OpenCV libraries.
- YOLOv8 CVI runtime inference and postprocess from `aka-sg2002/detect.*`.
- UART motor protocol from `aka-rk3588/motor/uart_motor_driver.*`.
- ZP10D arm UART protocol from `aka-rk3588/arm`.
- Tennis chasing and grab state machine from `aka-sg2002/tennis.cpp`.

`AKA-00/demo` and shell scripts are intentionally not rewritten here.

Host builds compile the pure Rust pieces and use a TPU stub. SG2002 builds link the TPU/OpenCV SDK automatically when the target architecture is `riscv64`, or when `AKARS_LINK_SG2002=1` is set.

Build for SG2002 (RISC-V musl, `riscv64gc-unknown-linux-musl`):

```bash
cd akars
./build-sg2002.sh
```

The script handles the cross-build details:

- Target is `riscv64gc-unknown-linux-musl`. The AKA-00 board runs a musl rootfs
  (loader `/lib/ld-musl-riscv64v0p7_xthead.so.1`) and the original C++ `aka0`
  project links the same TPU/OpenCV SDK with the musl toolchain. This is **not**
  glibc and **not** the bare-metal `-none-elf` target.
- That target's std is not installed via rustup, so std is built from source
  with `-Zbuild-std` (needs the `rust-src` component on a nightly toolchain).
- musl defaults to static-crt; the script forces dynamic linking so the binary
  uses the board's musl loader and the SDK's `.so` files, and overrides the
  dynamic-linker name to the board's `xthead` variant.
- The SDK's GNU `ld` (binutils 2.35) rejects the modern RISC-V ISA attributes
  emitted by current Rust/LLVM, so linking uses `rust-lld`.

Override the SDK / toolchain locations by exporting before calling:

```bash
TPU_SDK_PATH=/path/to/cvitek_tpu_sdk \
OPENCV_PATH=/path/to/cvitek_tpu_sdk/opencv \
TC_BIN=/path/to/host-tools/gcc/riscv64-linux-musl-x86_64/bin \
./build-sg2002.sh
```

The output binary is `target/riscv64gc-unknown-linux-musl/release/akars`. On the
device it needs the SDK's `libcviruntime.so`, `libcvikernel.so`, and
`libopencv_*.so.3.2` reachable via `LD_LIBRARY_PATH` (the original `aka0`
deployment already provides these).

Run on device:

```bash
./akars /path/to/yolo_model.cvimodel \
  --camera /dev/cvi-usb-camera0 \
  --motor /dev/ttyS3 \
  --arm /dev/ttyS2
```

Useful options:

- `--frames N`: stop after N frames for smoke tests.
- `--conf X`: confidence threshold, default `0.5`.
- `--iou X`: NMS IOU threshold, default `0.5`.
- `--classes N`: class count, default `1`.

Web control:

```bash
./akars serve \
  --listen 0.0.0.0:8080 \
  --motor /dev/ttyS3 \
  --arm /dev/ttyS2
```

Open `http://<device-ip>:8080/` to control the base and arm from the built-in frontend.
Use `--mock` to run the same frontend/backend on a development machine without AKA hardware.
