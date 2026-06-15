use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=AKARS_LINK_SG2002");
    println!("cargo:rerun-if-env-changed=TPU_SDK_PATH");
    println!("cargo:rerun-if-env-changed=OPENCV_PATH");
    println!("cargo:rerun-if-env-changed=CXX");
    println!("cargo:rerun-if-env-changed=AR");
    println!("cargo:rerun-if-changed=src/cv_bridge.cpp");
    println!("cargo:rustc-check-cfg=cfg(akars_sg2002)");

    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let force_link = env::var("AKARS_LINK_SG2002").ok().as_deref() == Some("1");
    if target_arch != "riscv64" && !force_link {
        return;
    }

    let sdk = env::var("TPU_SDK_PATH")
        .unwrap_or_else(|_| "/home/ajax/Proj/OS/sg2002/cvitek_tpu_sdk".to_string());
    let opencv = env::var("OPENCV_PATH").unwrap_or_else(|_| format!("{sdk}/opencv"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is set by cargo"));
    let obj = out_dir.join("cv_bridge.o");
    let lib = out_dir.join("libakars_cv_bridge.a");
    let cxx = env::var("CXX").unwrap_or_else(|_| {
        if target_arch == "riscv64" {
            "riscv64-unknown-linux-musl-g++".to_string()
        } else {
            "c++".to_string()
        }
    });
    let ar = env::var("AR").unwrap_or_else(|_| {
        if target_arch == "riscv64" {
            "riscv64-unknown-linux-musl-ar".to_string()
        } else {
            "ar".to_string()
        }
    });

    let status = Command::new(&cxx)
        .args(["-std=c++11", "-fPIC", "-O2", "-Wall", "-Wextra", "-I"])
        .arg(format!("{opencv}/include"))
        .args(["-c", "src/cv_bridge.cpp", "-o"])
        .arg(&obj)
        .status()
        .expect("failed to invoke CXX for OpenCV bridge");
    assert!(status.success(), "failed to compile OpenCV bridge");

    let status = Command::new(&ar)
        .args(["crs"])
        .arg(&lib)
        .arg(&obj)
        .status()
        .expect("failed to invoke ar for OpenCV bridge");
    assert!(status.success(), "failed to archive OpenCV bridge");

    println!("cargo:rustc-cfg=akars_sg2002");
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=akars_cv_bridge");
    println!("cargo:rustc-link-search=native={sdk}/lib");
    println!("cargo:rustc-link-search=native={opencv}/lib");
    println!("cargo:rustc-link-lib=dylib=cviruntime");
    println!("cargo:rustc-link-lib=dylib=cvikernel");
    println!("cargo:rustc-link-lib=dylib=opencv_core");
    println!("cargo:rustc-link-lib=dylib=opencv_imgcodecs");
    println!("cargo:rustc-link-lib=dylib=opencv_imgproc");
    println!("cargo:rustc-link-lib=dylib=stdc++");
    println!("cargo:rustc-link-lib=dylib=z");
    println!("cargo:rustc-link-lib=dylib=dl");
    println!("cargo:rustc-link-lib=dylib=pthread");
    println!("cargo:rustc-link-lib=dylib=atomic");
}
