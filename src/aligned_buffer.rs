use std::cmp::min;
use std::io::Read;

use crate::constants::{MIN_BUFFER_SIZE, CHUNK_SIZE};

#[repr(C, align(64))]
pub struct AlignedBuffer<T: Read> {
    buffer: Vec<u8>,
    start: usize,
    valid_bytes: usize,
    reader: T,
    line_start: usize,
    buffer_size: usize,
}

impl<T: Read> AlignedBuffer<T> {
    pub fn new(reader: T) -> Self {
        let mut new_buffer = AlignedBuffer {
            buffer: vec![0u8; MIN_BUFFER_SIZE],
            start: 0,
            valid_bytes: 0,
            reader,
            line_start: 0,
            buffer_size: MIN_BUFFER_SIZE,
        };
        new_buffer.fill_buf_initial();
        return new_buffer;
    }

    pub fn grow_buf(&mut self) {
        self.buffer_size *= 2;
        self.buffer.resize(self.buffer_size, 0);
    }

    pub fn compact(&mut self) {
        // compaction step--move the remaining bytes to the front of the buffer and read into the rest.
        // unsafe so we don't lose all our speed from bounds checks
        unsafe {
            std::ptr::copy(
                self.buffer.as_ptr().add(self.line_start),
                self.buffer.as_mut_ptr(),
                self.valid_bytes - self.line_start,
            );
        }
        let line_remaining = self.valid_bytes - self.line_start;
        let res = self.reader.read(
            unsafe {
                self.buffer.get_unchecked_mut(line_remaining..)
            }
        );
        match res {
            Ok(r) => {
                self.valid_bytes = line_remaining + r
            }
            Err(r) => {
                panic!("Error reading from input: {:?}", r);
            }
        }
        self.start = self.start - self.line_start;
        self.line_start = 0;
    }

    pub fn fill_buf_initial(&mut self) {
        let res = self.reader.read(
            unsafe {
                self.buffer.get_unchecked_mut(0..)
            }
        );
        match res {
            Ok(r) => {
                self.valid_bytes = r
            }
            Err(r) => {
                panic!("Error reading from input: {:?}", r);
            }
        }
    }

    pub fn get_chunk(&mut self) -> (&[u8], usize) {
        unsafe {
            std::hint::assert_unchecked(self.buffer.len() % 64 == 0);
        }
        // the amount of valid bytes before we need to start moving buffer
        let remaining = self.valid_bytes - self.start;
        if remaining < CHUNK_SIZE * 2 {
            // this means that the current line is nearing the size of the buffer, which means we need
            // to grow the max buffer size by doubling the size of the buffer.
            if (self.valid_bytes - self.line_start) > self.buffer_size / 2 {
                // println!("doubling buffer from {} to {}", self.buffer_size, self.buffer_size * 2);
                self.grow_buf();
            }
            self.compact();
        }
        return (unsafe { self.buffer.get_unchecked(self.start..self.start + CHUNK_SIZE) }, min(self.valid_bytes - self.start, CHUNK_SIZE));
    }

    pub fn start_line(&mut self) {
        self.line_start = self.start;
    }


    pub fn get_line_slice(&mut self) -> &[u8] {
        let ret = unsafe { self.buffer.get_unchecked(self.line_start..self.start) };
        if self.buffer[self.start] == b'\r' {
            self.start += 1;
        };
        self.start += 1;
        return ret;
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