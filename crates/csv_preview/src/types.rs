use std::fmt::Debug;

pub use coordinates::*;
mod coordinates;
pub use table_cell::*;
mod table_cell;
pub use table_like_content::*;
mod table_like_content;

/// Line number information for delimited text rows.
#[derive(Debug, Clone, Copy)]
pub enum LineNumber {
    /// Single logical source row number.
    Line(usize),
    /// Logical source row number range. Inclusive.
    LineRange(usize, usize),
}
