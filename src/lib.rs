#![feature(test)]

mod tests;
mod arch;
pub mod aligned_buffer;
mod constants;
mod record;

use crate::record::Record;
use std::io::Read;
use std::ops::Index;
use crate::aligned_buffer::AlignedBuffer;
use crate::constants::{CHUNK_SIZE};
use crate::arch::prefix_xor::clmul64;
use crate::arch::simd::{Classifier};


extern crate test;

pub struct Dialect {
    pub delimiter: char,
    pub quotechar: char,
    pub skipinitialspace: bool,
    pub strict: bool,
}

pub fn default_dialect() -> Dialect {
    return Dialect::new(
        ',',
        '\"',
        false,
        false,
    );
}

impl Dialect {
    pub fn new(delimiter: char, quotechar: char, skipinitialspace: bool, strict: bool) -> Self {
        return Dialect {
            delimiter,
            quotechar,
            skipinitialspace,
            strict,
        }
    }
}

thread_local! {
    static CLASSIFIER: Classifier = Classifier::new();
}
pub struct Parser<T: Read> {
    pub dialect: Dialect,
    pub inside_quotes: bool,
    pub bufreader: AlignedBuffer<T>,
    delimiters: Vec<usize>,
    classifier: Classifier,
}
impl<T: Read> Parser<T> {
    pub fn new(dialect: Dialect, bufreader: AlignedBuffer<T>) -> Self {
        return Parser {
            dialect: dialect,
            inside_quotes: false,
            bufreader: bufreader,
            delimiters: Vec::<usize>::new(),
            classifier: Classifier::new(),
        }
    }

    #[inline(always)]
    fn chunk_delimiter_offsets(quote_locations: u64, newline_locations: u64, delimiter_locations:u64, inside_quotes: bool) -> (u64, u64, u32) {
        let unescaped_quote_count = quote_locations.count_ones();

        // xor with current inside quotes state to get correct quote mask
        let quote_mask = quote_locations ^ inside_quotes as u64;
        let inside_quotes = !clmul64(!0u64, quote_mask);
        let filtered_delimiter_locations: u64 = delimiter_locations & inside_quotes;

        let filtered_newline_locations = newline_locations & inside_quotes;

        return (filtered_delimiter_locations, filtered_newline_locations, unescaped_quote_count)
    }

    fn reset_line_state(&mut self) {
        self.delimiters.clear();
        self.delimiters.push(0);
        self.bufreader.start_line();
        self.inside_quotes = false;
    }

    fn process_buffer_chunks(&mut self) -> Option<Record<'_>> {
        self.reset_line_state();
        let mut last_offset = 0;
        loop {
            // get the next chunk from the buffer, with n<=64 valid bytes
            let (chunk, mut n) = self.bufreader.get_chunk();
            if n == 0 {
                break
            }
            // find delimiters, quotes, newlines
            let (delimiter_locations, quote_locations, newline_locations) = self.classifier.classify(chunk);
            let (mut delimiter_offsets, mut newline_offsets, quote_count) = Self::chunk_delimiter_offsets(quote_locations, newline_locations, delimiter_locations, self.inside_quotes);
            let first_newline = newline_offsets.trailing_zeros() as usize;
            if quote_count % 2 != 0 {
                self.inside_quotes = !self.inside_quotes;
            }
            let mut last_delimiter_offset: usize = 0;
            // iterate over the offsets
            while delimiter_offsets != 0 {
                let pos = delimiter_offsets.trailing_zeros() as usize;
                if pos >= first_newline {
                    break
                }
                delimiter_offsets &= delimiter_offsets - 1;
                // +1 to include the comma, otherwise the offsets become misaligned
                last_offset += pos - last_delimiter_offset + 1;
                self.delimiters.push(last_offset);
                last_delimiter_offset = pos+1;
            }
            if first_newline != CHUNK_SIZE && first_newline <= n {
                last_offset += first_newline - last_delimiter_offset;
                self.delimiters.push(last_offset);
                self.bufreader.consume(first_newline);
                return Some(Record::new(
                    self.bufreader.get_line_slice(),
                    self.delimiters.as_slice(),
                ));
            }
            last_offset += n - last_delimiter_offset;
            self.bufreader.consume(n);
        }
        return None
    }
    pub fn read_line(&mut self) -> Option<Record<'_>> {
        return self.process_buffer_chunks();
    }
}



