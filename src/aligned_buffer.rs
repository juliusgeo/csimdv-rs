use std::cmp::min;
use std::io::Read;

use crate::constants::{BUFFER_SIZE, CHUNK_SIZE};
pub struct AlignedBuffer<T: Read> {
    buffer: [u8; BUFFER_SIZE],
    start: usize,
    valid_bytes: usize,
    reader: T,
}

impl<T: Read> AlignedBuffer<T> {
    pub fn new(reader: T) -> Self {
        let mut new_buffer = AlignedBuffer {
            buffer: [0u8; BUFFER_SIZE],
            start: 0,
            valid_bytes: 0,
            reader,
        };
        new_buffer.fill_buf_initial();
        return new_buffer;
    }

    pub fn fill_buf_initial(&mut self) {
        let res = self.reader.read(&mut self.buffer[..]);
        match res {
            Ok(r) => {
                self.valid_bytes = r
            }
            Err(r) => {
                panic!("Error reading from input: {:?}", r);
            }
        }
    }

    pub fn get_chunk(&mut self) -> ([u8; CHUNK_SIZE], usize) {
        let remaining = self.valid_bytes - self.start;
        if remaining < CHUNK_SIZE {
            self.buffer.copy_within(self.start..BUFFER_SIZE, 0);
            let res = self.reader.read(&mut self.buffer[remaining..]);
            match res {
                Ok(r) => {
                    self.valid_bytes = remaining + r
                }
                Err(r) => {
                    panic!("Error reading from input: {:?}", r);
                }
            }
            self.start = 0;
        }
        let arr: [u8; CHUNK_SIZE] = unsafe { self.buffer[self.start..self.start+CHUNK_SIZE].try_into().unwrap_unchecked() };
        return (arr, min(self.valid_bytes - self.start, CHUNK_SIZE));
    }

    pub fn consume(&mut self, amt: usize) {
        self.start += amt;
    }
}

#[cfg(test)]
mod buftests {
    use crate::aligned_buffer::AlignedBuffer;
    use std::io::{Cursor};
    fn cursor_from_str(s: &str) -> Cursor<&[u8]> {
        Cursor::new(s.as_bytes())
    }

    #[test]
    fn test_bufread() {
        let line = "1,2,30,\"300, 400\",4\n";
        let reader = cursor_from_str(line);
        let mut buf = AlignedBuffer::new(reader);
        let (chunk, valid_bytes) = buf.get_chunk();
        assert_eq!(&chunk[0..5], b"1,2,3");
        assert_eq!(valid_bytes, 20);
        buf.consume(5);
        let (chunk, valid_bytes) = buf.get_chunk();
        assert_eq!(valid_bytes, 15);
        assert_eq!(&chunk[0..5], b"0,\"30");
        buf.consume(14);
        let (chunk, valid_bytes) = buf.get_chunk();
        assert_eq!(valid_bytes, 1);
        assert_eq!(&chunk[0..1], b"\n");
    }
}