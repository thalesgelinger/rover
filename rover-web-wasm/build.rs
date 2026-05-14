fn main() {
    let target = std::env::var("TARGET").expect("TARGET missing");
    if !target.contains("emscripten") {
        return;
    }

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR missing");
    let target_path = std::path::PathBuf::from(out_dir)
        .parent()
        .expect("missing parent 1")
        .parent()
        .expect("missing parent 2")
        .parent()
        .expect("missing parent 3")
        .join("rover_web_wasm");

    println!(
        "cargo:rustc-link-arg=-sEXPORTED_RUNTIME_METHODS=['cwrap','ccall','FS','FS_createPath','FS_createDataFile']"
    );
    println!("cargo:rustc-link-arg=-sFORCE_FILESYSTEM=1");
    println!(
        "cargo:rustc-link-arg=-sEXPORTED_FUNCTIONS=['_rover_web_init','_rover_web_load_lua','_rover_web_tick','_rover_web_pull_html','_rover_web_dispatch_click','_rover_web_dispatch_input','_rover_web_dispatch_submit','_rover_web_dispatch_toggle','_rover_web_set_viewport','_rover_web_last_error','_rover_web_next_wake_ms']"
    );
    println!("cargo:rustc-link-arg=-sNO_DISABLE_EXCEPTION_CATCHING");
    println!("cargo:rustc-link-arg=-sASSERTIONS=1");
    println!("cargo:rustc-link-arg=-sEXPORT_ES6=1");
    println!("cargo:rustc-link-arg=-sMODULARIZE=1");
    println!("cargo:rustc-link-arg=-sSINGLE_FILE=1");
    println!("cargo:rustc-link-arg=--no-entry");
    println!(
        "cargo:rustc-link-arg=-o{}.js",
        target_path.to_string_lossy()
    );
}
