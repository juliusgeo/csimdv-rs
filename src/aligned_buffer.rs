#[cfg(not(windows))]
pub use mmap_buffer::AlignedBuffer;
#[cfg(windows)]
pub use read_buffer::AlignedBuffer;

#[cfg(not(windows))]
mod mmap_buffer {
    use crate::constants::CHUNK_SIZE;
    use crate::map::mmap::MappedFile;

    pub struct AlignedBuffer {
        mmap: MappedFile,
        start: usize,
        line_start: usize,
    }

    impl AlignedBuffer {
        pub fn new(file: &std::fs::File) -> std::io::Result<Self> {
            Ok(AlignedBuffer { start: 0, line_start: 0, mmap: MappedFile::new(file)? })
        }

        pub fn get_chunk(&mut self) -> (&[u8], usize) {
            let n = CHUNK_SIZE.min(self.mmap.len() - self.start);
            (&self.mmap[self.start..self.start + n], n)
        }

        pub fn start_line(&mut self) {
            self.line_start = self.start;
        }

        pub fn get_tail_slice(&self) -> &[u8] {
            &self.mmap[self.line_start..self.start]
        }

        pub fn get_line_slice(&mut self) -> &[u8] {
            let ret = &self.mmap[self.line_start..self.start];
            if self.mmap[self.start] == b'\r' {
                self.start += 1;
            }
            self.start += 1;
            ret
        }

        pub fn consume(&mut self, amt: usize) {
            self.start += amt;
        }
    }
}

#[cfg(windows)]
mod read_buffer {
    use std::fs::File;
    use std::io::{ErrorKind, Read};

    use crate::constants::{BUFFER_SIZE, CHUNK_SIZE};

    pub struct AlignedBuffer {
        buffer: Vec<u8>,
        reader: File,
        start: usize,
        line_start: usize,
        valid_bytes: usize,
        buffer_size: usize,
        eof: bool,
    }

    impl AlignedBuffer {
        pub fn new(file: &File) -> std::io::Result<Self> {
            Self::with_capacity(file, BUFFER_SIZE)
        }

        pub(crate) fn with_capacity(file: &File, buffer_size: usize) -> std::io::Result<Self> {
            let mut buf = AlignedBuffer {
                buffer: vec![0; buffer_size + CHUNK_SIZE],
                reader: file.try_clone()?,
                start: 0,
                line_start: 0,
                valid_bytes: 0,
                buffer_size,
                eof: false,
            };
            buf.refill();
            Ok(buf)
        }

        fn grow(&mut self) {
            self.buffer_size *= 2;
            self.buffer.resize(self.buffer_size + CHUNK_SIZE, 0);
        }

        fn refill(&mut self) {
            self.buffer.copy_within(self.line_start..self.valid_bytes, 0);
            self.valid_bytes -= self.line_start;
            self.start -= self.line_start;
            self.line_start = 0;
            while !self.eof && self.valid_bytes < self.buffer_size {
                match self.reader.read(&mut self.buffer[self.valid_bytes..self.buffer_size]) {
                    Ok(0) => self.eof = true,
                    Ok(n) => self.valid_bytes += n,
                    Err(e) if e.kind() == ErrorKind::Interrupted => {}
                    Err(e) => panic!("Error reading from input: {e:?}"),
                }
            }
            // the previous fill's bytes would otherwise show up as delimiters.
            self.buffer[self.valid_bytes..self.valid_bytes + CHUNK_SIZE].fill(0);
        }

        #[inline(always)]
        pub fn get_chunk(&mut self) -> (&[u8], usize) {
            // +2 leaves room for the "\r\n" that get_line_slice may step over.
            if !self.eof && self.valid_bytes - self.start < CHUNK_SIZE + 2 {
                // the current line fills more than half the buffer: compacting
                // would leave too little room to read into, so grow first.
                if self.valid_bytes - self.line_start > self.buffer_size / 2 {
                    self.grow();
                }
                self.refill();
            }
            let n = CHUNK_SIZE.min(self.valid_bytes - self.start);
            (&self.buffer[self.start..self.start + CHUNK_SIZE], n)
        }

        pub fn start_line(&mut self) {
            self.line_start = self.start;
        }

        pub fn get_tail_slice(&self) -> &[u8] {
            &self.buffer[self.line_start..self.start]
        }

        pub fn get_line_slice(&mut self) -> &[u8] {
            let ret = &self.buffer[self.line_start..self.start];
            if self.buffer[self.start] == b'\r' {
                self.start += 1;
            }
            self.start += 1;
            ret
        }

        pub fn consume(&mut self, amt: usize) {
            self.start += amt;
        }
    }
}

#[cfg(test)]
mod buftests {
    use crate::aligned_buffer::AlignedBuffer;
    use std::io::{Write};
    fn reader_from_str(s: &str) -> AlignedBuffer {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(s.as_bytes()).unwrap();
        f.flush().unwrap();
        AlignedBuffer::new(&f.reopen().unwrap()).unwrap()
    }

    #[test]
    fn test_bufread() {
        let line = "1,2,30,\"300, 400\",4\n";
        let mut buf = reader_from_str(line);
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

    #[test]
    fn test_block_past_eof_is_readable_zeros() {
        let mut buf = reader_from_str("a,b\n");
        buf.consume(3);
        let (chunk, valid_bytes) = buf.get_chunk();
        assert_eq!(valid_bytes, 1);
        assert_eq!(chunk[0], b'\n');
        assert!(chunk[1..].iter().all(|&b| b == 0));
    }

    #[test]
    fn test_empty_file() {
        let mut buf = reader_from_str("");
        let (chunk, valid_bytes) = buf.get_chunk();
        assert_eq!(valid_bytes, 0);
        assert!(chunk.iter().all(|&b| b == 0));
    }
}
