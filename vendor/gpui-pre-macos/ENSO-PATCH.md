# Enso compatibility patch

Source: gpui-pre-macos 0.3.10 from crates.io, Apache-2.0.

In `src/window.rs`, three calls to unstable `Option::map_or_default(f)` are replaced by `Option::map_or(Default::default(), f)`.  The values involved are native rectangle/size structures, whose defaults have no side effects.

Also in `src/window.rs`, the blurred window backdrop (`WindowBackgroundAppearance::Blurred`) uses `NSVisualEffectMaterial::Sidebar` instead of `UnderWindowBackground`, and `blurred_view_update_layer` no longer strips the material's desktop tint and saturation. Enso's sidebar is transparent over it, so it gets the native macOS sidebar tint. Keep this when upgrading.

Remove this Cargo patch when the matching GPUI Kit release supports stable Rust without it.
