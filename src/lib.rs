#![feature(portable_simd)]
#![feature(test)]

mod tests;
mod macros;
pub mod aligned_buffer;
mod constants;
use lender::*;
use std::cmp::min;
use std::fmt;
use std::io::Read;
use std::simd::{Mask, Simd};
use std::simd::cmp::SimdPartialEq;
use std::ops::Index;
use crate::aligned_buffer::AlignedBuffer;
use crate::constants::{CHUNK_SIZE};
extern crate test;


pub struct Record<'a> {
    data: &'a [u8],
    offsets: &'a [usize],
    current_field: usize,
}

impl<'a> Record<'a> {
    pub fn new() -> Self {
        return Record {
            data: &[],
            offsets: &[],
            current_field: 0,
        }
    }

    pub fn len(&self) -> usize {
        return self.offsets.len()-1;
    }
}
impl<'a> fmt::Debug for Record<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for i in 0..self.len() {
            if i != 0 {
                write!(f, ", ")?;
            }
            write!(f, "\"{}\"", &self[i])?;
        }
        Ok(())
    }
}
impl<'a> Index<usize> for Record<'a> {
    type Output = str;
    fn index(&self, index: usize) -> &Self::Output {

        let (start, mut end) = (self.offsets[index], self.offsets[index+1]);
        if index < self.len() - 1 {
            end -= 1;
        }
        return str::from_utf8(&self.data[start..end]).unwrap();
    }
}

impl<'a> PartialEq<Vec<&str>> for Record<'a> {
    fn eq(&self, other: &Vec<&str>) -> bool {
        if self.len() != other.len() {
            return false
        }
        for i in 0..self.len() {
            if &self[i] != other[i] {
                return false
            }
        }
        return true
    }

}

impl<'lend, 'a> Lending<'lend> for Record<'a> {
    type Lend = &'lend str;
}
impl<'a> Lender for Record<'a> {
    fn next(&mut self) -> Option<&'_ str> {
        if self.offsets.len() == 0 || !(self.current_field < self.offsets.len() -1){
            return None
        }
        let (start, end) = (self.offsets[self.current_field], self.offsets[self.current_field+1]);
        self.current_field += 1;
        Some(str::from_utf8(&self.data[start..end]).unwrap())
    }
}


#[derive(Clone, Copy)]
pub struct Dialect {
    pub delimiter: char,
    pub quotechar: char,
    pub skipinitialspace: bool,
    pub strict: bool,
}

pub fn default_dialect() -> Dialect {
    return Dialect {
        delimiter: ',',
        quotechar: '\"',
        skipinitialspace: false,
        strict: false,
    }
}




pub struct Parser<T: Read> {
    pub dialect: Dialect,
    pub inside_quotes: bool,
    pub bufreader: AlignedBuffer<T>,
    data: Vec<u8>,
    delimiters: Vec<usize>,
}
impl<T: Read> Parser<T> {
    pub fn new(dialect: Dialect, bufreader: AlignedBuffer<T>) -> Self {
        return Parser {
            dialect: dialect,
            inside_quotes: false,
            bufreader: bufreader,
            data: Vec::<u8>::new(),
            delimiters: Vec::<usize>::new(),
        }
    }

    fn mask_invalid_bytes(valid_bytes: usize) -> u64 {
        if valid_bytes >= 64 {
            return !0u64;
        }
        let mask_limit = 1 << (valid_bytes-1);
        let mask = mask_limit & (!mask_limit + 1);
        return mask | (mask - 1);
    }

    fn chunk_delimiter_offsets(chunk: [u8; CHUNK_SIZE], dialect: Dialect, inside_quotes: bool) -> (u64, usize, u32) {
        // create the simd line
        let simd_line:Simd<u8, CHUNK_SIZE> = Simd::from_array(chunk);

        // find delimiters and quotes
        let delimiter_locations = simd_line.simd_eq(Simd::splat(dialect.delimiter as u8));
        let quote_locations = simd_line.simd_eq(Simd::splat(dialect.quotechar as u8));
        let mut quote_locations_mask = quote_locations.to_bitmask();
        let unescaped_quote_count = quote_locations_mask.count_ones();

        // xor with current inside quotes state to get correct quote mask
        let quote_mask = quote_locations_mask ^ inside_quotes as u64;
        let inside_quotes = clmul64!(!0u64, quote_mask) as u64;
        let filtered_delimiter_locations: u64 = delimiter_locations.to_bitmask() & !inside_quotes;

        // calculate where newlines are
        let newline_locations = simd_line.simd_eq(Simd::splat(b'\n'));
        let return_locations = simd_line.simd_eq(Simd::splat(b'\r'));
        let newline_return_locations: Mask<i8, 64> = return_locations & newline_locations.shift_elements_right::<1>(true);

        let all_newline_locations = newline_locations | newline_return_locations | return_locations;
        let filtered_newline_locations: u64 = all_newline_locations.to_bitmask() & !inside_quotes;

        // ignore any delimiter offsets past the newline
        let first_newline = filtered_newline_locations.trailing_zeros() as usize;
        return (filtered_delimiter_locations, first_newline, unescaped_quote_count)
    }


    fn process_buffer_chunks(&mut self) -> Option<Record<'_>> {
        self.data.clear();
        self.delimiters.clear();
        self.delimiters.push(0);
        // to minimize data copies, we keep track of the current offset for each delimiter.
        // if the chunk has no newline, we copy over the whole thing. If it has a newline, copy up till the newline.
        let mut last_offset = 0;
        loop {
            // get the next chunk from the buffer, with n<=64 valid bytes
            let (chunk, n) = self.bufreader.get_chunk();
            if n == 0 {
                break
            }
            let (mut delimiter_offsets, first_newline, quote_count) = Self::chunk_delimiter_offsets(chunk, self.dialect, self.inside_quotes);
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
                self.data.extend_from_slice(&chunk[0..first_newline]);
                last_offset += first_newline - last_delimiter_offset;
                self.delimiters.push(last_offset);
                self.bufreader.consume(min(n, first_newline+1));
                return Some(Record {
                    data: self.data.as_slice(),
                    offsets: self.delimiters.as_slice(),
                    current_field: 0,
                });
            }
            self.data.extend_from_slice(&chunk[0..n]);
            last_offset += n - last_delimiter_offset;
            self.bufreader.consume(n);
        }
        return None
    }
    pub fn read_line(&mut self) -> Option<Record<'_>> {
        return self.process_buffer_chunks();
    }
}



