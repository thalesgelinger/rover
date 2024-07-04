use std::{env, process::Command};

fn main() {
    if cfg!(feature = "android") {
        build_android();
    } else if cfg!(feature = "ios") {
        build_ios();
    } else {
        println!("No specific build script feature enabled.");
    }
}

fn build_ios() {
    let targets = [
        "aarch64-apple-ios",
        "aarch64-apple-ios-sim",
        "x86_64-apple-ios",
    ];

    for target in &targets {
        Command::new("cargo")
            .args(&["build", "--release", "--target", target])
            .status()
            .expect("Failed to build for aarch64-apple-ios");
    }

    Command::new("cbindgen")
        .args(&[
            "--lang",
            "c",
            "--crate",
            "gears",
            "--output",
            "target/gears.h",
        ])
        .status()
        .expect("Failed to generate C bindings");

    Command::new("lipo")
        .args(&[
            "-create",
            "-output",
            "target/release/libgears.a",
            "target/aarch64-apple-ios-sim/release/libgears.a",
            "target/x86_64-apple-ios/release/libgears.a",
        ])
        .status()
        .expect("cargo:warning=Failed to create universal library");

    Command::new("cp")
        .args(&["target/ios/libgears.a", "../ios/ios/Gears/"])
        .status()
        .expect("Failed to copy .a to ios Gears");

    Command::new("cp")
        .args(&["target/gears.h", "../ios/ios/Gears/"])
        .status()
        .expect("Failed to copy .h to ios Gears");
}

fn build_android() {
    println!("cargo:warning=Running Android script");

    let ndk_path = env::var("ANDROID_NDK_HOME").expect("ANDROID_NDK_HOME not set");
    let target = env::var("TARGET").unwrap();
    println!("cargo:warning=NDK Path: {}", ndk_path);
    println!("cargo:warning=Target: {}", target);

    if target.contains("android") {
        let lib_path = match target.as_str() {
            "aarch64-linux-android" => "aarch64-linux-android/lib",
            "armv7-linux-androideabi" => "arm-linux-androideabi/lib",
            "i686-linux-android" => "i686-linux-android/lib",
            "x86_64-linux-android" => "x86_64-linux-android/lib",
            _ => panic!("Unknown android target: {}", target),
        };

        println!(
            "cargo:rustc-link-search=native={}/sources/cxx-stl/llvm-libc++/libs/{}",
            ndk_path, lib_path
        );
    }

    let targets = [
        "aarch64-linux-android",
        "armv7-linux-androideabi",
        "i686-linux-android",
        "x86_64-linux-android",
    ];

    for target in &targets {
        Command::new("cargo")
            .args(&[
                "ndk",
                "--target",
                target,
                "--platform",
                "31",
                "--",
                "build",
                "--release",
            ])
            .status()
            .expect(&format!("Failed to build for {}", target));
    }

    let android_targets = [
        ("aarch64-linux-android", "arm64-v8a"),
        ("armv7-linux-androideabi", "armeabi-v7a"),
        ("i686-linux-android", "x86"),
        ("x86_64-linux-android", "x86_64"),
    ];

    for (from, to) in &android_targets {
        Command::new("cp")
            .args(&[
                format!("target/{}/release/libgears.so", from),
                format!("../android/app/src/main/jniLibs/{}/", to),
            ])
            .status()
            .expect(&format!("Failed to copy {} to {}", from, to));
    }
}
