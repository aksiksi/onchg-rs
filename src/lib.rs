mod file;
mod git;
mod parser;
pub mod test_helpers;

pub use file::{OnChangeBlock, ThenChange, ThenChangeTarget, ON_CHANGE_PAT_STR};
pub use parser::{OnChangeViolation, Parser};
