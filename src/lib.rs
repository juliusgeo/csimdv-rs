#![feature(portable_simd)]
#![feature(test)]

use std::cmp::{max, min};
use std::io::{BufRead, BufReader, Read};
use std::simd::Simd;
use std::simd::cmp::SimdPartialEq;
extern crate test;

const CHUNK_SIZE: usize = 64;

const BUFFER_SIZE: usize = CHUNK_SIZE * 1024;
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

pub struct Parser<T: Read> {
    pub dialect: Dialect,
    pub inside_quotes: bool,
    pub bufreader: BufReader<T>,
}
impl<T: Read> Parser<T> {
    pub fn new(dialect: Dialect, bufreader: BufReader<T>) -> Self {
        return Parser {
            dialect: dialect,
            inside_quotes: false,
            bufreader: bufreader,
        }
    }

    fn chunk_delimiter_offsets(&self, chunk: &[u8; CHUNK_SIZE], valid_bytes: usize) -> (u64, u64, u64) {
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
        if valid_bytes < 64 {
            let mask_limit = 1 << (valid_bytes-1);
            let mask = mask_limit & (!mask_limit + 1);
            filtered_delimiter_locations = filtered_delimiter_locations & (mask | (mask - 1));
            filtered_quote_locations = filtered_quote_locations & (mask | (mask - 1));
        }
        return (filtered_delimiter_locations, filtered_newline_locations, filtered_quote_locations)
    }

    fn escape_quotes(&self, s: String) -> String {
        return s.replace(&format!("{}{}", self.dialect.quotechar, self.dialect.quotechar), &self.dialect.quotechar.to_string());
    }

    fn field_to_string(&self, field_buf: &mut Vec<u8>) -> Option<String> {
        if field_buf.len() > 1 && field_buf[0] == self.dialect.quotechar as u8 {
            field_buf.remove(0);
            field_buf.remove(field_buf.len()-1);
        }
        match String::from_utf8(std::mem::take(field_buf)) {
            Ok(v) => Some(self.escape_quotes(v)),
            Err(e) => None,
        }
    }
    fn process_buffer_chunks(&mut self) -> Vec<String> {
        let mut new_tokens = Vec::<String>::new();
        let mut chunk = [0u8; CHUNK_SIZE];
        let mut field_buf = Vec::<u8>::new();
        while true {
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

            let (mut delimiter_offsets, newline_offsets, quote_offsets) = self.chunk_delimiter_offsets(&chunk, n);
            let mut delimiter_positions = Vec::new();
            let first_newline = newline_offsets.trailing_zeros() as usize;
            let quote_count = quote_offsets.count_ones() as usize;
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
                field_buf.extend_from_slice(&chunk[last_delimiter_offset..i]);
                new_tokens.push(self.field_to_string(&mut field_buf).expect("Invalid UTF-8 sequence"));
                last_delimiter_offset = i+1;
            }
            if first_newline != 64 {
                field_buf.extend_from_slice(&chunk[last_delimiter_offset..first_newline]);
                new_tokens.push(self.field_to_string(&mut field_buf).expect("Invalid UTF-8 sequence"));
                self.bufreader.consume(min(n, first_newline+1));
                return new_tokens;
            }
            if quote_count % 2 != 0 {
                self.inside_quotes = !self.inside_quotes;
            }
            field_buf.extend_from_slice(&chunk[last_delimiter_offset..n]);
            self.bufreader.consume(n);
        }
        return new_tokens
    }
    pub fn parse_buffer(&mut self) -> Vec<String> {
        return self.process_buffer_chunks()
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
        let result = p.parse_buffer();
        assert_eq!(result, vec!["1".to_string(), "2".to_string(), "30".to_string(), "300, 400".to_string(),  "4".to_string()])
    }

    #[test]
    fn test_line_parsing_continuation() {
        let line = ", \",1,2,\"300, 400\",4\n";
        let mut p = Parser {
            dialect: default_dialect(),
            inside_quotes: true,
            bufreader: reader_from_str(line),
        };
        let result = p.parse_buffer();
        assert_eq!(result, vec![", \"".to_string(), "1".to_string(), "2".to_string(), "300, 400".to_string(),  "4".to_string()])
    }

    #[test]
    fn test_line_parsing_escaped_newlines() {
        let line = ", \",1,2,\"300,\r\n 400\",4\n";
        let mut p = Parser {
            dialect: default_dialect(),
            inside_quotes: true,
            bufreader: reader_from_str(line),
        };
        let result = p.parse_buffer();
        assert_eq!(result, vec![", \"".to_string(), "1".to_string(), "2".to_string(), "300,\r\n 400".to_string(),  "4".to_string()])
    }

    #[test]
    fn test_line_parsing_boundaries() {
        let line = "12345678910,12345678910,12345678910,12345678910,offscore blah blah,season\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let result = p.parse_buffer();
        println!("{:?}", result);
        assert_eq!(result[result.len() -2], "offscore blah blah".to_string())
    }


    #[test]
    fn test_line_parsing_nfl_1() {
        let line = "20120905_DAL@NYG,1,,0,DAL,NYG,,,,D.Bailey kicks 69 yards from DAL 35 to NYG -4. D.Wilson to NYG 16 for 20 yards (A.Holmes).,0,0,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let result = p.parse_buffer();
        println!("{:?}", result);
        assert_eq!(result[result.len()-4], "D.Bailey kicks 69 yards from DAL 35 to NYG -4. D.Wilson to NYG 16 for 20 yards (A.Holmes).".to_string())
    }

    #[test]
    fn test_line_parsing_nfl_2() {
        let line = "20120905_DAL@NYG,1,59,49,NYG,DAL,2,10,84,(14:49) E.Manning pass short middle to V.Cruz to NYG 21 for 5 yards (S.Lee) [J.Hatcher].,0,0,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let result = p.parse_buffer();
        println!("{:?}", result);
        assert_eq!(result[result.len()-4], "(14:49) E.Manning pass short middle to V.Cruz to NYG 21 for 5 yards (S.Lee) [J.Hatcher].".to_string())
    }

    #[test]
    fn test_line_parsing_nfl_3() {
        let line = "20120905_DAL@NYG,1,57,9,NYG,DAL,1,10,87,(12:09) A.Bradshaw left tackle to NYG 15 for 2 yards (J.Hatcher J.Price-Brent).,0,0,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let result = p.parse_buffer();
        println!("{:?}", result);
        assert_eq!(result[result.len()-4], "(12:09) A.Bradshaw left tackle to NYG 15 for 2 yards (J.Hatcher J.Price-Brent).".to_string());
        assert_eq!(result[result.len()-1], "2012");
    }

    #[test]
    fn test_line_parsing_nfl_4() {
        let line = "20120905_DAL@NYG,1,57,9,NYG,DAL,1,10,87,(12:09) A.Bradshaw left tackle to NYG 15 for 2 yards (J.Hatcher J.Price-Brent).,0,0,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let result = p.parse_buffer();
        println!("{:?}", result);
        assert_eq!(result[result.len()-4], "(12:09) A.Bradshaw left tackle to NYG 15 for 2 yards (J.Hatcher J.Price-Brent).".to_string());
        assert_eq!(result[result.len()-1], "2012");
    }

    #[test]
    fn test_line_parsing_nfl_nested_quotes() {
        let line = "20120923_TB@DAL,3,29,12,TB,DAL,3,8,78,\"(14:12) (Shotgun) J.Freeman pass incomplete deep left to D.Clark. Pass incomplete on a \"\"seam\"\" route; Carter closest defender.\",7,10,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let result = p.parse_buffer();
        println!("{:?}", result);
        assert_eq!(result[result.len()-4], "(14:12) (Shotgun) J.Freeman pass incomplete deep left to D.Clark. Pass incomplete on a \"seam\" route; Carter closest defender.".to_string());
        assert_eq!(result[result.len()-1], "2012");
    }
    #[test]
    fn test_parse_file() {
        let file = File::open("examples/nfl.csv").unwrap();
        let mut p = Parser::new(default_dialect(), BufReader::new(file));
        let result = p.parse_buffer();
        println!("{:?}", result);
        for _ in 0..10000 {
            let result = p.parse_buffer();
            println!("{:?}", result);
        }
        // assert_eq!(result, vec![", \"", "1", "2", "\"300, 400\"",  "4"])
    }

    #[bench]
    fn bench_parse_file(b: &mut Bencher) {
        let file = File::open("examples/nfl.csv").unwrap();
        let mut p = Parser::new(default_dialect(), BufReader::new(file));
        let result = p.parse_buffer();
        println!("{:?}", result);
        fn parse_file(p: &mut Parser<File>){
            for _ in 0..10000 {
                p.parse_buffer();
            }
        }
        b.iter(|| parse_file(&mut p));
    }


}
