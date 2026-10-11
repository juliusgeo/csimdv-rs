#![feature(test)]

mod arch;
mod constants;
mod tests;

pub mod aligned_buffer;
pub mod de;
pub mod parser;
pub mod reader;
pub mod record;
#[cfg(not(windows))]
mod map;

pub use parser::{Dialect, Parser, default_dialect};
pub use reader::{ByteReader, SerdeReader};

extern crate test;
