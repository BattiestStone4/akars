use std::env;
use std::path::{Path, PathBuf};

const DEFAULT_TPU_SDK_DIR: &str = "toolchains/tpu-sdk-sg200x";

fn main() {
    println!("cargo:rerun-if-env-changed=AKARS_TPU_SDK_DIR");
    println!("cargo:rerun-if-env-changed=AKARS_LINK_SG2002");

    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if target_arch != "riscv64" {
        return;
    }

    // TPU SDK (cviruntime/cvikernel) 链接改为 opt-in:仅当显式设置
    // AKARS_LINK_SG2002=1 时才链。serve / 摄像头路径不碰 TPU(UsbCamera 是纯
    // ioctl),默认跳过,避免要求空的 TPU SDK submodule,产出零 cvitek 依赖的
    // 二进制。hunt/detect 等需要 TPU 的路径设置 AKARS_LINK_SG2002=1 即恢复链接。
    if env::var("AKARS_LINK_SG2002").as_deref() != Ok("1") {
        // 通知 tpu.rs:不走 riscv64 CVI FFI 实现,用纯 Rust stub(无 extern CVI 符号)
        println!("cargo:rustc-cfg=akars_no_tpu");
        return;
    }

    let manifest_dir = env_path("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let tpu_sdk =
        env_path("AKARS_TPU_SDK_DIR").unwrap_or_else(|| manifest_dir.join(DEFAULT_TPU_SDK_DIR));
    let tpu_sdk_include = tpu_sdk.join("include");
    let tpu_sdk_lib = tpu_sdk.join("lib");

    require_dir("TPU SDK include", &tpu_sdk_include);
    require_dir("TPU SDK library", &tpu_sdk_lib);

    println!(
        "cargo:rerun-if-changed={}",
        tpu_sdk_include.join("cviruntime.h").display()
    );
    println!("cargo:rustc-link-search=native={}", tpu_sdk_lib.display());
    println!("cargo:rustc-link-lib=dylib=cviruntime");
    println!("cargo:rustc-link-lib=dylib=cvikernel");
    println!("cargo:rustc-link-lib=dylib=stdc++");
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn require_dir(label: &str, path: &Path) {
    assert!(
        path.is_dir(),
        "{label} directory does not exist: {}",
        path.display()
    );
}
