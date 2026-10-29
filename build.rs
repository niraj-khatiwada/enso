fn main() {
    // Embed the app icon in enso.exe (see assets/enso.rc).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=assets/enso.rc");
        println!("cargo:rerun-if-changed=assets/enso.ico");
        embed_resource::compile("assets/enso.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("embedding the Windows icon");
    }
}
