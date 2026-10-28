# Enso compatibility patch

Source: gpui-pre-macos 0.3.10 from crates.io, Apache-2.0.

In `src/window.rs`, three calls to unstable `Option::map_or_default(f)` are replaced by `Option::map_or(Default::default(), f)`. No other upstream source changes. The values involved are native rectangle/size structures, whose defaults have no side effects.

Remove this Cargo patch when the matching GPUI Kit release supports stable Rust without it.
