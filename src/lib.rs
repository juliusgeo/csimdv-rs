#![feature(portable_simd)]
#![feature(test)]

mod tests;
mod macros;
pub mod aligned_buffer;
mod constants;
mod record;

use std::arch::aarch64::vld4q_u8;
use crate::record::Record;
use std::cmp::min;
use std::io::Read;
use std::simd::{Simd};
use std::simd::cmp::SimdPartialEq;
use std::ops::Index;
use crate::aligned_buffer::AlignedBuffer;
use crate::constants::{CHUNK_SIZE};
extern crate test;

#[derive(Clone, Copy)]
pub struct Splats {
    pub delimiter: Simd<u8, CHUNK_SIZE>,
    pub quotechar: Simd<u8, CHUNK_SIZE>,
    pub newline: Simd<u8, CHUNK_SIZE>,
    pub returnchar: Simd<u8, CHUNK_SIZE>,
}

#[derive(Clone, Copy)]
pub struct Dialect {
    pub delimiter: char,
    pub quotechar: char,
    pub skipinitialspace: bool,
    pub strict: bool,
    pub splats: Splats,
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
        let a: Simd<u8, CHUNK_SIZE> = Simd::splat(delimiter as u8);
        let b: Simd<u8, CHUNK_SIZE> = Simd::splat(quotechar as u8);
        let c: Simd<u8, CHUNK_SIZE> = Simd::splat(b'\n');
        let d: Simd<u8, CHUNK_SIZE> = Simd::splat(b'\r');
        let splats = Splats {
            delimiter: a,
            quotechar: b,
            newline: c,
            returnchar: d,
        };
        return Dialect {
            delimiter,
            quotechar,
            skipinitialspace,
            strict,
            splats: splats,
        }
    }
}

pub struct Parser<T: Read> {
    pub dialect: Dialect,
    pub inside_quotes: bool,
    pub bufreader: AlignedBuffer<T>,
    delimiters: Vec<usize>,
}
impl<T: Read> Parser<T> {
    pub fn new(dialect: Dialect, bufreader: AlignedBuffer<T>) -> Self {
        return Parser {
            dialect: dialect,
            inside_quotes: false,
            bufreader: bufreader,
            delimiters: Vec::<usize>::new(),
        }
    }

    #[inline(always)]
    fn chunk_delimiter_offsets(chunk: &[u8], dialect: Dialect, inside_quotes: bool) -> (u64, usize, u32, usize) {
        // create the simd line
        let chunk_simd = Simd::<u8, CHUNK_SIZE>::from_slice(chunk);
        // find delimiters and quotes
        let (delimiter_locations, quote_locations, newline_locations, return_locations) = simd_eq_bitmask!(chunk_simd, dialect.splats.delimiter, dialect.splats.quotechar, dialect.splats.newline, dialect.splats.returnchar);

        let quote_locations_mask = quote_locations;
        let unescaped_quote_count = quote_locations_mask.count_ones();

        // xor with current inside quotes state to get correct quote mask
        let quote_mask = quote_locations_mask ^ inside_quotes as u64;
        let inside_quotes = !clmul64!(!0u64, quote_mask) as u64;
        let filtered_delimiter_locations: u64 = delimiter_locations & inside_quotes;

        // calculate where newlines are
        let newline_return_locations = newline_locations << 1 & return_locations;

        let filtered_newline_locations_size_1: u64 = (newline_locations | return_locations) & inside_quotes;
        let filtered_newline_locations_size_2: u64 = newline_return_locations & inside_quotes;

        // ignore any delimiter offsets past the newline
        let first_newline_size_1 = filtered_newline_locations_size_1.trailing_zeros() as usize;
        let first_newline_size_2 = filtered_newline_locations_size_2.trailing_zeros() as usize;
        if first_newline_size_1 < first_newline_size_2 {
            return (filtered_delimiter_locations, first_newline_size_1, unescaped_quote_count, 1)
        }
        return (filtered_delimiter_locations, first_newline_size_2, unescaped_quote_count, 2)
    }

    fn reset_line_state(&mut self) {
        self.delimiters.clear();
        self.delimiters.push(0);
        self.bufreader.start_line();
        self.inside_quotes = false;
    }

    fn process_buffer_chunks(&mut self) -> Option<Record<'_>> {
        self.reset_line_state();
        // to minimize data copies, we keep track of the current offset for each delimiter.
        // if the chunk has no newline, we copy over the whole thing. If it has a newline, copy up till the newline.
        let mut last_offset = 0;
        loop {
            // get the next chunk from the buffer, with n<=64 valid bytes
            let (chunk, n) = self.bufreader.get_chunk();
            if n == 0 {
                break
            }
            let (mut delimiter_offsets, first_newline, quote_count, newline_size) = Self::chunk_delimiter_offsets(chunk, self.dialect, self.inside_quotes);
            if first_newline <= newline_size && self.delimiters.len() == 1{
                self.bufreader.consume(newline_size);
                self.reset_line_state();
                continue;
            }
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
                self.bufreader.consume(min(n, first_newline));
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



