use std::{env, fs::File, io::Write, path::Path};

fn main() {
    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rerun-if-changed=build.rs");

    // cortex-m-rt linker script
    println!("cargo::rustc-link-arg=-Tlink.x");
    // defmt linker script
    println!("cargo::rustc-link-arg=-Tdefmt.x");

    // embedded-test
    println!("cargo::rustc-link-arg=-Tembedded-test.x");
    println!("cargo::rustc-check-cfg=cfg(rust_analyzer)");

    copy_memory_linker_script()
}

fn copy_memory_linker_script() {
    let out_dir = env::var("OUT_DIR").expect("No OUT_DIR");
    let dest_path = Path::new(&out_dir);
    let mut f = File::create(dest_path.join("memory.x")).unwrap();
    f.write_all(include_bytes!("memory.x")).unwrap();

    println!("cargo:rustc-link-search={}", dest_path.display());
}
