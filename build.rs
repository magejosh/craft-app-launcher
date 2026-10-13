fn main() {
    println!("cargo:rerun-if-changed=app.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref()==Ok("windows") && std::env::var("CARGO_CFG_TARGET_ENV").as_deref()==Ok("msvc") {
        let manifest=std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("app.manifest");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        // Cargo/rustc pass this as one argument; embedded quotes become literal to mt.exe.
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}",manifest.display());
    }
}