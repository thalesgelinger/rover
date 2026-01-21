mod traits;
mod tui;
#[cfg(test)]
pub mod test_utils;

pub use traits::Renderer;
pub use tui::{TuiRenderer, run_tui};

#[cfg(test)]
pub use test_utils::TestRenderer;
