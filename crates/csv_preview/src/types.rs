use std::fmt::Debug;

pub use coordinates::*;
mod coordinates;
pub use table_cell::*;
mod table_cell;
pub use table_like_content::*;
mod table_like_content;

/// Line number information for CSV rows
#[derive(Debug, Clone, Copy)]
pub enum LineNumber {
    /// Single logical CSV row number
    Line(usize),
    /// Logical CSV row number range. Inclusive.
    LineRange(usize, usize),
}
