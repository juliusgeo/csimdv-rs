#![feature(portable_simd)]
#![feature(test)]

mod tests;
mod macros;

use lender::*;
use std::cmp::{min};
use std::fmt;
use std::io::{BufRead, BufReader, Read};
use std::simd::Simd;
use std::simd::cmp::SimdPartialEq;
use std::ops::{Index};
extern crate test;

const CHUNK_SIZE: usize = 64;

const MAX_FIELD_SIZE: usize = 1 << 17;
pub struct Record {
    data: Vec<u8>,
    offsets: Vec<(usize, usize)>,
    num_fields: usize,
    current_field: usize,
    in_field: bool,
}

impl Record {
    pub fn new() -> Self {
        return Record {
            data: Vec::<u8>::new(),
            offsets: Vec::<(usize, usize)>::new(),
            num_fields: 0,
            current_field: 0,
            in_field: false,
        }
    }

    pub fn clear(&mut self) {
        self.data.clear();
        self.offsets.clear();
        self.num_fields = 0;
        self.current_field = 0;
        self.in_field = false
    }

    pub fn append_field(&mut self, field: &[u8]) {
        let mut start = self.data.len();
        if self.in_field {
            let (start_offset, _) = self.offsets.remove(self.offsets.len()-1);
            start = start_offset;
            self.in_field = false;
        }
        self.data.extend_from_slice(field);
        let mut end = self.data.len();
        self.offsets.push((start, end));
        self.num_fields += 1;
    }

    pub fn append_field_chunk(&mut self, field_chunk: &[u8]) {
        let mut start = self.data.len();
        self.data.extend_from_slice(field_chunk);
        let end = self.data.len();
        if !self.in_field {
            self.in_field = true;
            self.offsets.push((start, end));
        } else {
            let (start, _) = self.offsets.remove(self.offsets.len()-1);
            self.offsets.push((start, end) );
        }
    }

    pub fn len(&self) -> usize {
        return self.num_fields;
    }
}
impl fmt::Debug for Record {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for i in 0..self.num_fields {
            if i != 0 {
                write!(f, ", ")?;
            }
            write!(f, "\"{}\"", &self[i])?;
        }
        Ok(())
    }
}
impl Index<usize> for Record {
    type Output = str;
    fn index(&self, index: usize) -> &Self::Output {
        let (start, end) = self.offsets[index];
        return str::from_utf8(&self.data[start..end]).unwrap();
    }
}

impl PartialEq<Vec<&str>> for Record {
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

impl<'lend> Lending<'lend> for Record {
    type Lend = &'lend str;
}
impl Lender for Record {
    fn next(&mut self) -> Option<&'_ str> {
        if self.offsets.len() == 0 {
            return None
        }
        let (start, end) = self.offsets.remove(0);
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

    pub fn to_slice(&self) -> &[u8] {
        let (mut start, mut end) = (self.start_offset, self.end_offset);
        if self.buf[start] == self.dialect.quotechar as u8 {
            start += 1;
            end -= 1;
        }
        return &self.buf[start..end];
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


    fn process_buffer_chunks(&mut self, record: &mut Record) -> bool {
        record.clear();
        let mut chunk = [0u8; CHUNK_SIZE];
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

            let (mut delimiter_offsets, mut first_newline, quote_count) = Self::chunk_delimiter_offsets(&chunk, n, self.dialect, self.inside_quotes);
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
                record.append_field(&chunk[last_delimiter_offset..pos]);
                last_delimiter_offset = pos+1;
                delimiter_offsets &= delimiter_offsets - 1;
            }
            if first_newline != 64 {
                record.append_field(&chunk[last_delimiter_offset..first_newline]);
                self.bufreader.consume(min(n, first_newline+1));
                return true;
            }
            record.append_field_chunk(&chunk[last_delimiter_offset..n]);
            self.bufreader.consume(n);
        }
        return false
    }
    pub fn read_line(&mut self, record: &mut Record) -> bool {
        return self.process_buffer_chunks(record);
    }
}



