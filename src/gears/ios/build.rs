use std::{env, process::Command};

fn main() {
    let targets = [
        "aarch64-apple-ios",
        "aarch64-apple-ios-sim",
        "x86_64-apple-ios",
    ];

    let current_dir = env::current_dir().expect("Failed to get current directory");

    let rover_ios_path = current_dir
        .parent()
        .expect("No parent directory")
        .parent()
        .expect("No parent directory")
        .join("RoverIos")
        .join("RoverIos")
        .join("Gears");

    println!("cargo:warning=building for targets");

    for target in &targets {
        Command::new("cargo")
            .args(&["build", "--release", "--target", target])
            .status()
            .expect("Failed to build for aarch64-apple-ios");
        println!("cargo:warning=Build for {}", target);
    }

    println!("cargo:warning=Generating header");

    // Command::new("cbindgen")
    //     .args(&[
    //         "--lang",
    //         "c",
    //         "--crate",
    //         "gears",
    //         "--output",
    //         "../target/gears.h",
    //     ])
    //     .status()
    //     .expect("Failed to generate C bindings");

    // println!("cargo:warning=Grouping .a");

    // Command::new("lipo")
    //     .args(&[
    //         "-create",
    //         "-output",
    //         "../target/libgears.a",
    //         "../target/aarch64-apple-ios-sim/release/libgears.a",
    //         "../target/x86_64-apple-ios/release/libgears.a",
    //     ])
    //     .status()
    //     .expect("cargo:warning=Failed to create universal library");

    // println!("cargo:warning=Coping files");

    // Command::new("cp")
    //     .args(&["../target/libgears.a", &rover_ios_path.to_string_lossy()])
    //     .status()
    //     .expect("cargo:warning=Failed to copy .a to RoverIos");

    // Command::new("cp")
    //     .args(&["../target/gears.h", &rover_ios_path.to_string_lossy()])
    //     .status()
    //     .expect("cargo:warning=Failed to copy .a to RoverIos");

    // println!("cargo:warning=Done");

    // let current_dir = env::current_dir().expect("Failed to get current directory");

    // let rover_ios_path = current_dir
    //     .parent()
    //     .expect("No parent directory")
    //     .join("RoverIos")
    //     .join("RoverIos.xcodeproj");

    // Command::new("xcodebuild")
    //     .args(&[
    //         "-project",
    //         &rover_ios_path.to_string_lossy(),
    //         "-scheme",
    //         "RoverIos",
    //         "-configuration",
    //         "Release",
    //         "-sdk",
    //         "iphonesimulator",
    //         "CONFIGURATION_BUILD_DIR=build",
    //         "build",
    //     ])
    //     .status()
    //     .expect("cargo:warning=Failed to build the Swift framework");
}
