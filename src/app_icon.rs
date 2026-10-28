/// Set the Dock icon for `cargo run`, where no .app bundle supplies one.
#[cfg(target_os = "macos")]
pub fn init() {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    let data = NSData::with_bytes(include_bytes!("../assets/enso.png"));
    if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
        // AppKit's setter requires a non-null image; the decoded image is live
        // and the application callback runs on the macOS main thread.
        unsafe {
            NSApplication::sharedApplication(main_thread).setApplicationIconImage(Some(&image));
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn init() {}
