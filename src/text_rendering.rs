#[cfg(target_os = "macos")]
pub fn apply(crisp: bool) {
    use objc2_foundation::{NSUserDefaults, ns_string};
    let defaults = NSUserDefaults::standardUserDefaults();
    let key = ns_string!("AppleFontSmoothing");
    if crisp {
        defaults.setInteger_forKey(0, key);
    } else {
        defaults.removeObjectForKey(key);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn apply(_crisp: bool) {}
