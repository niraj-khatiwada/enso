mod build;
pub mod caret;
mod commands;
pub mod document;
mod input;
pub mod mapping;
pub mod perf;
pub mod rows;
#[cfg(all(test, feature = "ui-tests"))]
mod tests;
pub mod view;
