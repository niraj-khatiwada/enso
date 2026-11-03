mod build;
pub mod caret;
mod commands;
pub mod document;
mod focus;
mod input;
pub mod mapping;
pub mod perf;
pub mod prefs;
pub mod rows;
#[cfg(all(test, feature = "ui-tests"))]
mod tests;
pub mod view;
