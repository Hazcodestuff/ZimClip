fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("app-resource.o");
    let tool = std::env::var("WINDRES").unwrap_or_else(|_| "x86_64-w64-mingw32-windres".into());
    let status = std::process::Command::new(tool)
        .args(["-i", "assets/app.rc", "-O", "coff", "-o"])
        .arg(&out)
        .status()
        .expect("windres is required to embed the application icon");
    assert!(status.success());
    println!("cargo:rustc-link-arg={}", out.display());
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/zimclip.ico");
    for i in 0..8 {
        println!("cargo:rerun-if-changed=assets/accent-{i}.ico");
    }
}
