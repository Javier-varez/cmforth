use std::{env, fs::File, io::Write, path::Path};

fn main() {
    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rerun-if-changed=build.rs");
    copy_linker_script()
}

fn copy_linker_script() {
    let out_dir = env::var("OUT_DIR").expect("No OUT_DIR");
    let dest_path = Path::new(&out_dir);
    let mut f = File::create(dest_path.join("memory.x")).unwrap();
    f.write_all(include_bytes!("memory.x")).unwrap();

    println!("cargo:rustc-link-search={}", dest_path.display());
}
