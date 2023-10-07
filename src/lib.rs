mod core;
mod git;
mod parser;
pub mod test_helpers;

pub use core::{OnChangeBlock, ThenChange, ThenChangeTarget};
pub use parser::{OnChangeViolation, Parser};
