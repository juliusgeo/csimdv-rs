use crate::aligned_buffer::AlignedBuffer;
use crate::arch::prefix_xor::clmul64;
use crate::arch::simd::Classifier;
use crate::constants::CHUNK_SIZE;
use crate::record::Record;

#[derive(Clone, Copy)]
pub struct Dialect {
    pub delimiter: char,
    pub quotechar: char,
    pub skipinitialspace: bool,
    pub strict: bool,
    pub escape: bool,
    pub escape_char: char,
}

pub fn default_dialect() -> Dialect {
    Dialect::new(
        ',',
        '\"',
        false,
        false,
        true,
        '"'
    )
}

impl Dialect {
    pub fn new(delimiter: char, quotechar: char, skipinitialspace: bool, strict: bool, escape: bool, escape_char: char) -> Self {
        return Dialect {
            delimiter,
            quotechar,
            skipinitialspace,
            strict,
            escape,
            escape_char,
        }
    }
}

pub struct Parser {
    pub dialect: Dialect,
    pub inside_quotes: bool,
    pub bufreader: AlignedBuffer,
    delimiters: Vec<usize>,
    classifier: Classifier,
}
impl Parser {
    pub fn new(dialect: Dialect, bufreader: AlignedBuffer) -> Self {
        Parser {
            dialect: dialect,
            inside_quotes: false,
            bufreader: bufreader,
            delimiters: Vec::<usize>::new(),
            classifier: Classifier::new(dialect),
        }
    }

    #[inline(always)]
    fn chunk_delimiter_offsets(quote_locations: u64, newline_locations: u64, delimiter_locations:u64, inside_quotes: bool) -> (u64, u64, u32) {
        // get the current quote mask xored with either 0 or u64::MAX. The intuition is that if
        // inside quotes is true, we essentially flip the quote mask -- inside becomes outside, outside
        // becomes inside. Significantly faster than oring inside_quotes with the first bit of quote_locations
        // prior to the clmul64 call because it means that quote_locations can stay in a vector register.
        let prefix = clmul64(!0u64, quote_locations) ^ 0u64.wrapping_sub(inside_quotes as u64);
        let outside_quotes = !prefix;

        let filtered_delimiter_locations: u64 = delimiter_locations & outside_quotes;
        let filtered_newline_locations = newline_locations & outside_quotes;

        let ends_inside_quotes = (prefix >> 63) as u32 ^ inside_quotes as u32;
        (filtered_delimiter_locations, filtered_newline_locations, ends_inside_quotes)
    }

    fn reset_line_state(&mut self) {
        self.delimiters.clear();
        // the separator before the first field
        // when iterating, we subtract one from the first of the delimiter pairs,
        // wrapping_sub from MAX is 0
        self.delimiters.push(usize::MAX);
        self.bufreader.start_line();
        self.inside_quotes = false;
    }

    fn process_buffer_chunks(&mut self) -> Option<Record<'_>> {
        self.reset_line_state();
        let mut off = 0;
        loop {
            // get the next chunk from the buffer, with n<=64 valid bytes
            let (chunk, n) = self.bufreader.get_chunk();
            if n == 0 {
                if off == 0 {
                    return None; // clean EOF
                }
                // EOF without a trailing newline: everything since the line start is the last record.
                self.delimiters.push(off);
                return Some(Record::new(self.bufreader.get_tail_slice(), self.delimiters.as_slice()));
            }
            // find delimiters, quotes, newlines
            let (delimiter_locations, quote_locations, newline_locations) = self.classifier.classify(chunk);
            let (mut delimiter_offsets,  newline_offsets, ends_inside_quotes) = Self::chunk_delimiter_offsets(quote_locations, newline_locations, delimiter_locations, self.inside_quotes);
            let first_newline = newline_offsets.trailing_zeros() as usize;
            // only the delimiters before the first newline (all of them when there is none)
            delimiter_offsets &= (newline_offsets & newline_offsets.wrapping_neg()).wrapping_sub(1);
            // iterate over the offsets
            while delimiter_offsets != 0 {
                let pos = delimiter_offsets.trailing_zeros() as usize;
                delimiter_offsets &= delimiter_offsets - 1;
                self.delimiters.push(pos + off);
            }
            if first_newline != CHUNK_SIZE && first_newline <= n {
                self.delimiters.push(first_newline + off);
                self.bufreader.consume(first_newline);
                return Some(Record::new(
                    self.bufreader.get_line_slice(),
                    self.delimiters.as_slice(),
                ));
            }
            if ends_inside_quotes != 0 {
                self.inside_quotes = !self.inside_quotes;
            }
            off += n;
            self.bufreader.consume(n);
        }
    }
    pub fn read_line(&mut self) -> Option<Record<'_>> {
        self.process_buffer_chunks()
    }
}
