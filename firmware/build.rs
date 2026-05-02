use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    // Copy memory.x to the linker search path (OUT_DIR).
    // The embassy/cortex-m-rt link script includes it from there.
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));

    let memory_x = fs::read("memory.x").expect("cannot read memory.x");
    fs::write(out_dir.join("memory.x"), memory_x).expect("cannot write memory.x to OUT_DIR");

    // Tell cargo to re-run this build script if memory.x changes.
    println!("cargo:rerun-if-changed=memory.x");

    // Tell the linker to search OUT_DIR for link scripts.
    println!("cargo:rustc-link-search={}", out_dir.display());
}
