#![feature(portable_simd)]
#![feature(test)]

use std::cmp::{min};
use std::io::{BufRead, BufReader, Read};
use std::simd::Simd;
use std::simd::cmp::SimdPartialEq;
extern crate test;

const CHUNK_SIZE: usize = 64;

const MAX_FIELD_SIZE: usize = 1 << 17;
#[macro_export]
macro_rules! clmul64 {
    ($a:expr, $b:expr) => {{
        #[cfg(all(target_arch = "x86_64", target_feature = "pclmulqdq"))]
        unsafe {
            use core::arch::x86_64::*;
            let va = _mm_set_epi64x(0, $a as i64);
            let vb = _mm_set_epi64x(0, $b as i64);
            let r = _mm_clmulepi64_si128(va, vb, 0x00);
            r
        }

        #[cfg(all(target_arch = "aarch64", target_feature = "aes"))]
        unsafe {
            use core::arch::aarch64::*;
            let r = vmull_p64($a, $b);
            r
        }

        #[cfg(not(any(
            all(target_arch = "x86_64", target_feature = "pclmulqdq"),
            all(target_arch = "aarch64", target_feature = "aes")
        )))]
        compile_error!("CLMUL not supported on this architecture");
    }};
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

    fn mask_invalid_bytes(&self,valid_bytes: usize) -> u64 {
        if valid_bytes >= 64 {
            return !0u64;
        }
        let mask_limit = 1 << (valid_bytes-1);
        let mask = mask_limit & (!mask_limit + 1);
        return mask | (mask - 1);
    }

    fn chunk_delimiter_offsets(&self, chunk: &[u8; CHUNK_SIZE], valid_bytes: usize) -> (u64, usize, u32) {
        let simd_line:Simd<u8, CHUNK_SIZE> = Simd::from_array(*chunk);
        let delimiter_locations = simd_line.simd_eq(Simd::splat(self.dialect.delimiter as u8));
        let quote_locations = simd_line.simd_eq(Simd::splat(self.dialect.quotechar as u8));
        // xor with current inside quotes state to get correct quote mask
        let quote_mask = quote_locations.to_bitmask() ^ self.inside_quotes as u64;
        let inside_quotes = clmul64!(!0u64, quote_mask) as u64;
        let mut filtered_delimiter_locations: u64 = delimiter_locations.to_bitmask() & !inside_quotes;

        let newline_locations = simd_line.simd_eq(Simd::splat(b'\n')).to_bitmask();
        let return_locations = simd_line.simd_eq(Simd::splat(b'\r')).to_bitmask();
        let newline_return_locations = return_locations & newline_locations >> 1;
        let all_newline_locations = newline_locations | newline_return_locations | return_locations;
        let filtered_newline_locations: u64 = all_newline_locations & !inside_quotes;
        let mut filtered_quote_locations = quote_locations.to_bitmask();
        // ignore any delimiter offsets past the newline
        let mut mask = self.mask_invalid_bytes(valid_bytes);
        let first_newline = filtered_newline_locations.trailing_zeros() as usize;
        // if we have a newline we want to mask out any delimiters/quotes past it
        if filtered_newline_locations != 0 {
            if first_newline != 0 {
                let newline_mask = self.mask_invalid_bytes(first_newline);
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

            let (mut delimiter_offsets, first_newline, quote_count) = self.chunk_delimiter_offsets(&chunk, n);
            let mut delimiter_positions = Vec::new();
            if quote_count % 2 != 0 {
                self.inside_quotes = !self.inside_quotes;
            }
            while delimiter_offsets != 0 {
                let pos = delimiter_offsets.trailing_zeros() as usize;
                if pos >= first_newline {
                    break
                }
                delimiter_positions.push(pos);
                delimiter_offsets &= delimiter_offsets - 1;
            }
            let mut last_delimiter_offset: usize = 0;
            for i in delimiter_positions {
                let diff = i - last_delimiter_offset;
                self.field_buffer.append(&chunk[last_delimiter_offset..i], diff);
                new_tokens.push(self.field_buffer.to_escaped_string().expect("Invalid UTF-8 sequence"));
                last_delimiter_offset = i+1;
                self.field_buffer.clear();
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


#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::{BufReader, Cursor};
    use super::*;
    use test::Bencher;
    fn reader_from_str(s: &str) -> BufReader<Cursor<&[u8]>> {
        BufReader::new(Cursor::new(s.as_bytes()))
    }

    #[test]
    fn test_line_parsing() {
        let line = "1,2,30,\"300, 400\",4\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let result = p.read_line();
        assert_eq!(result, vec!["1".to_string(), "2".to_string(), "30".to_string(), "300, 400".to_string(),  "4".to_string()])
    }

    #[test]
    fn test_line_parsing_continuation() {
        let line = ", \",1,2,\"300, 400\",4\n";
        let mut p = Parser {
            dialect: default_dialect(),
            inside_quotes: true,
            bufreader: reader_from_str(line),
            field_buffer: FieldBuffer::new(default_dialect()),
        };
        let result = p.read_line();
        assert_eq!(result, vec![", \"".to_string(), "1".to_string(), "2".to_string(), "300, 400".to_string(),  "4".to_string()])
    }

    #[test]
    fn test_line_parsing_escaped_newlines() {
        let line = ", \",1,2,\"300,\r\n 400\",4\n";
        let mut p = Parser {
            dialect: default_dialect(),
            inside_quotes: true,
            bufreader: reader_from_str(line),
            field_buffer: FieldBuffer::new(default_dialect()),
        };
        let result = p.read_line();
        assert_eq!(result, vec![", \"".to_string(), "1".to_string(), "2".to_string(), "300,\r\n 400".to_string(),  "4".to_string()])
    }

    #[test]
    fn test_line_parsing_boundaries() {
        let line = "12345678910,12345678910,12345678910,12345678910,offscore blah blah,season\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let result = p.read_line();
        println!("{:?}", result);
        assert_eq!(result[result.len() -2], "offscore blah blah".to_string())
    }


    #[test]
    fn test_line_parsing_nfl_1() {
        let line = "20120905_DAL@NYG,1,,0,DAL,NYG,,,,D.Bailey kicks 69 yards from DAL 35 to NYG -4. D.Wilson to NYG 16 for 20 yards (A.Holmes).,0,0,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let result = p.read_line();
        println!("{:?}", result);
        assert_eq!(result[result.len()-4], "D.Bailey kicks 69 yards from DAL 35 to NYG -4. D.Wilson to NYG 16 for 20 yards (A.Holmes).".to_string())
    }

    #[test]
    fn test_line_parsing_nfl_2() {
        let line = "20120905_DAL@NYG,1,59,49,NYG,DAL,2,10,84,(14:49) E.Manning pass short middle to V.Cruz to NYG 21 for 5 yards (S.Lee) [J.Hatcher].,0,0,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let result = p.read_line();
        println!("{:?}", result);
        assert_eq!(result[result.len()-4], "(14:49) E.Manning pass short middle to V.Cruz to NYG 21 for 5 yards (S.Lee) [J.Hatcher].".to_string())
    }

    #[test]
    fn test_line_parsing_nfl_3() {
        let line = "20120905_DAL@NYG,1,57,9,NYG,DAL,1,10,87,(12:09) A.Bradshaw left tackle to NYG 15 for 2 yards (J.Hatcher J.Price-Brent).,0,0,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let result = p.read_line();
        println!("{:?}", result);
        assert_eq!(result[result.len()-4], "(12:09) A.Bradshaw left tackle to NYG 15 for 2 yards (J.Hatcher J.Price-Brent).".to_string());
        assert_eq!(result[result.len()-1], "2012");
    }

    #[test]
    fn test_line_parsing_nfl_4() {
        let line = "20120905_DAL@NYG,1,57,9,NYG,DAL,1,10,87,(12:09) A.Bradshaw left tackle to NYG 15 for 2 yards (J.Hatcher J.Price-Brent).,0,0,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let result = p.read_line();
        println!("{:?}", result);
        assert_eq!(result[result.len()-4], "(12:09) A.Bradshaw left tackle to NYG 15 for 2 yards (J.Hatcher J.Price-Brent).".to_string());
        assert_eq!(result[result.len()-1], "2012");
    }

    #[test]
    fn test_line_parsing_nfl_nested_quotes() {
        let line = "20120923_TB@DAL,3,29,12,TB,DAL,3,8,78,\"(14:12) (Shotgun) J.Freeman pass incomplete deep left to D.Clark. Pass incomplete on a \"\"seam\"\" route; Carter closest defender.\",7,10,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let result = p.read_line();
        assert_eq!(result[result.len()-4], "(14:12) (Shotgun) J.Freeman pass incomplete deep left to D.Clark. Pass incomplete on a \"seam\" route; Carter closest defender.".to_string());
        assert_eq!(result[result.len()-1], "2012");
    }

    #[test]
    fn test_delimiter_masking() {
        let line = "";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let mask = p.mask_invalid_bytes(16);
        assert_eq!(mask & 1 << 17 , 0);
    }
    #[test]
    fn test_parse_file() {
        let file = File::open("examples/nfl.csv").unwrap();
        let p = Parser::new(default_dialect(), BufReader::new(file));
        for (idx, line) in p.enumerate() {
            let _ = line;
            assert_ne!(line.len(), 0);

        }
    }

    #[bench]
    fn bench_parse_file(b: &mut Bencher) {
        fn parse_file(){
            let file = File::open("examples/nfl.csv").unwrap();
            let p = Parser::new(default_dialect(), BufReader::new(file));
            for line in p {
                let _ = line;
            }
        }
        b.iter(|| parse_file());
    }

    #[bench]
    fn bench_parse_line(b: &mut Bencher) {
        let file = File::open("examples/nfl.csv").unwrap();
        let p = Parser::new(default_dialect(), BufReader::new(file));
        let mut pp = p.into_iter();
        b.iter(|| pp.next());
    }


}
