use memmap2::Mmap;
use crate::constants::CHUNK_SIZE;

pub struct AlignedBuffer {
    mmap: Mmap,
    start: usize,
    line_start: usize,
}

impl AlignedBuffer {
    pub fn new(file: &std::fs::File) -> std::io::Result<Self> {
        let mmap = unsafe { Mmap::map(file)? };
        mmap.advise(memmap2::Advice::Sequential)?;
        Ok(AlignedBuffer {
            mmap,
            start: 0,
            line_start: 0,
        })
    }

    pub fn get_chunk(&mut self) -> (&[u8], usize) {
        let n = CHUNK_SIZE.min(self.mmap.len() - self.start);
        return (&self.mmap[self.start..self.start + n], n);
    }

    pub fn start_line(&mut self) {
        self.line_start = self.start;
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
}