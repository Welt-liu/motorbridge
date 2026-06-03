use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=src/dm_device_shim.cpp");
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo_root = manifest_dir.parent().unwrap();
    let dm_include = repo_root.join("third_party/dm_device/v1.1.0");

    cc::Build::new()
        .cpp(true)
        .file("src/dm_device_shim.cpp")
        .include(dm_include)
        .flag_if_supported("-std=c++17")
        .compile("dm_device_shim");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-lib=dylib=dl");
    }
}
