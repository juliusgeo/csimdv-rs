#![feature(portable_simd)]
#![feature(test)]

mod tests;
mod macros;

use std::cmp::{min};
use std::io::{BufRead, BufReader, Read};
use std::simd::Simd;
use std::simd::cmp::SimdPartialEq;
extern crate test;

const CHUNK_SIZE: usize = 64;

const MAX_FIELD_SIZE: usize = 1 << 17;

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


pub struct FieldBuffer {
    buf: [u8; MAX_FIELD_SIZE],
    end_offset: usize,
    start_offset: usize,
    dialect: Dialect
}

// struct to hold the contents of an individual field--with a buffer backing that is not reallocated on every line read
impl FieldBuffer {
    pub fn new(dialect: Dialect) -> Self {
        return FieldBuffer {
            buf: [0u8; MAX_FIELD_SIZE],
            end_offset: 0,
            start_offset: 0,
            dialect: dialect,
        }
    }

    pub fn clear(&mut self) {
        self.end_offset = 0;
        self.start_offset = 0;
    }

    pub fn append(&mut self, data: &[u8], n_bytes: usize) {
        if self.end_offset + n_bytes >= MAX_FIELD_SIZE {
            panic!("Field size exceeds maximum allowed size");
        }
        self.buf[self.end_offset..self.end_offset +n_bytes].copy_from_slice(data);
        self.end_offset += n_bytes;
    }
    fn escape_quotes(&self, s: String) -> String {
        return s.replace(&format!("{}{}", self.dialect.quotechar, self.dialect.quotechar), &self.dialect.quotechar.to_string());
    }
    pub fn to_string(&self) -> Option<String> {
        match str::from_utf8(&self.buf[self.start_offset..self.end_offset]) {
            Ok(v) => Some(v.to_string()),
            Err(e) => panic!("Invalid UTF-8 sequence: {}", e),
        }
    }

    pub fn to_escaped_string(&mut self) -> Option<String> {
        if self.end_offset > 1 && self.buf[self.start_offset] == self.dialect.quotechar as u8 && self.buf[self.end_offset -1] == self.dialect.quotechar as u8 {
            self.start_offset += 1;
            self.end_offset -= 1;
            let unescaped = self.escape_quotes(self.to_string().unwrap());
            return Some(unescaped);
        }
        return self.to_string()
    }
}


pub struct Parser<T: Read> {
    pub dialect: Dialect,
    pub inside_quotes: bool,
    pub bufreader: BufReader<T>,
    field_buffer: FieldBuffer,
}
impl<T: Read> Parser<T> {
    pub fn new(dialect: Dialect, bufreader: BufReader<T>) -> Self {
        return Parser {
            dialect: dialect,
            inside_quotes: false,
            bufreader: bufreader,
            field_buffer: FieldBuffer::new(dialect),
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

    fn chunk_delimiter_offsets(chunk: &[u8; CHUNK_SIZE], valid_bytes: usize, dialect: Dialect, inside_quotes: bool) -> (u64, usize, u32) {
        let simd_line:Simd<u8, CHUNK_SIZE> = Simd::from_slice(chunk);
        let delimiter_locations = simd_line.simd_eq(Simd::splat(dialect.delimiter as u8));
        let quote_locations = simd_line.simd_eq(Simd::splat(dialect.quotechar as u8));
        // xor with current inside quotes state to get correct quote mask
        let quote_mask = quote_locations.to_bitmask() ^ inside_quotes as u64;
        let inside_quotes = clmul64!(!0u64, quote_mask) as u64;
        let mut filtered_delimiter_locations: u64 = delimiter_locations.to_bitmask() & !inside_quotes;

        let newline_locations = simd_line.simd_eq(Simd::splat(b'\n')).to_bitmask();
        let return_locations = simd_line.simd_eq(Simd::splat(b'\r')).to_bitmask();
        let newline_return_locations = return_locations & newline_locations >> 1;
        let all_newline_locations = newline_locations | newline_return_locations | return_locations;
        let filtered_newline_locations: u64 = all_newline_locations & !inside_quotes;
        let mut filtered_quote_locations = quote_locations.to_bitmask();
        // ignore any delimiter offsets past the newline
        let mut mask = Self::mask_invalid_bytes(valid_bytes);
        let first_newline = filtered_newline_locations.trailing_zeros() as usize;
        // if we have a newline we want to mask out any delimiters/quotes past it
        if filtered_newline_locations != 0 {
            if first_newline != 0 {
                let newline_mask = Self::mask_invalid_bytes(first_newline);
                mask &= newline_mask;
            }
        }
        filtered_delimiter_locations = filtered_delimiter_locations & mask;
        filtered_quote_locations = filtered_quote_locations & mask;

        return (filtered_delimiter_locations, first_newline, filtered_quote_locations.count_ones())
    }


    fn process_buffer_chunks(&mut self) -> Vec<String> {
        let mut new_tokens = Vec::<String>::new();
        let mut chunk = [0u8; CHUNK_SIZE];
        self.field_buffer.clear();
        loop {
            // fill up the buffer and copy to chunk
            let b = self.bufreader.fill_buf();
            if b.is_ok() == false {
                break;
            }
            let buffer = b.unwrap();

            if buffer.len() == 0 {
                break;
            }
            // only copy at max CHUNK_SIZE bytes
            let n = min(buffer.len(), CHUNK_SIZE);
            chunk[0..n].copy_from_slice(&buffer[0..n]);

            let (mut delimiter_offsets, first_newline, quote_count) = Self::chunk_delimiter_offsets(&chunk, n, self.dialect, self.inside_quotes);
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
                let diff = pos - last_delimiter_offset;
                self.field_buffer.append(&chunk[last_delimiter_offset..pos], diff);
                new_tokens.push(self.field_buffer.to_escaped_string().expect("Invalid UTF-8 sequence"));
                last_delimiter_offset = pos+1;
                self.field_buffer.clear();
                delimiter_offsets &= delimiter_offsets - 1;
            }
            if first_newline != 64 {
                let diff = first_newline - last_delimiter_offset;
                self.field_buffer.append(&chunk[last_delimiter_offset..first_newline], diff);
                new_tokens.push(self.field_buffer.to_escaped_string().expect("Invalid UTF-8 sequence"));
                self.bufreader.consume(min(n, first_newline+1));
                return new_tokens;
            }
            let diff = n - last_delimiter_offset;
            self.field_buffer.append(&chunk[last_delimiter_offset..n], diff);
            self.bufreader.consume(n);
        }
        return new_tokens
    }
    pub fn read_line(&mut self) -> Vec<String> {
        return self.process_buffer_chunks()
    }
}

impl<T: Read> Iterator for Parser<T> {
    type Item = Vec<String>;
    fn next(&mut self) -> Option<Self::Item> {
        let record = self.read_line();
        if record.len() == 0 {
            return None;
        }
        return Some(record);
    }
}



