/// Set the Dock icon for `cargo run`, where no .app bundle supplies one.
/// Inside enso.app the bundle's icon stays, so macOS 26 can render its
/// light, dark, clear and tinted styles.
#[cfg(target_os = "macos")]
pub fn init() {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::{NSBundle, NSData};

    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    if NSBundle::mainBundle().bundleIdentifier().is_some() {
        return;
    }
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
