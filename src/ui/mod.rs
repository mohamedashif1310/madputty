//! UI modules: split-pane terminal renderer.

pub mod split_pane;

// Re-export for external consumers (integration tests, benchmarks).
#[allow(unused_imports)]
pub use split_pane::SplitPaneRenderer;
