fn main() {
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=assets/launcher/launcher.rc");
    println!("cargo:rerun-if-changed=assets/launcher/craft-app-launcher.ico");
    println!("cargo:rerun-if-env-changed=RC");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref()==Ok("windows") && std::env::var("CARGO_CFG_TARGET_ENV").as_deref()==Ok("msvc") {
        let manifest=std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("app.manifest");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        // Cargo/rustc pass this as one argument; embedded quotes become literal to mt.exe.
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}",manifest.display());
        let root=manifest.parent().unwrap();
        let resource=std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("launcher.res");
        let compiler=std::env::var_os("RC").unwrap_or_else(||"rc.exe".into());
        let status=std::process::Command::new(compiler).current_dir(root)
            .arg("/nologo").arg("/fo").arg(&resource).arg("assets/launcher/launcher.rc")
            .status().expect("Windows SDK rc.exe is required; run through build.ps1 or set RC to its full path");
        assert!(status.success(),"Windows launcher icon resource compilation failed");
        println!("cargo:rustc-link-arg-bin=craft-app-launcher={}",resource.display());
    }
}
