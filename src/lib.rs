mod file;
mod git;
mod parser;
pub mod test_helpers;

pub use file::{OnChangeBlock, ThenChange, ThenChangeTarget};
pub use parser::{OnChangeViolation, Parser};
