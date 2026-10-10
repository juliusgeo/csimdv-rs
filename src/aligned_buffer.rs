use std::os::fd::AsRawFd;

use crate::constants::CHUNK_SIZE;

pub struct AlignedBuffer {
    mmap: &'static [u8],
    // total size mapped, including zero buffer, for drop
    map_len: usize,
    start: usize,
    line_start: usize,
}

impl AlignedBuffer {
    pub fn new(file: &std::fs::File) -> std::io::Result<Self> {
        let len = file.metadata()?.len() as usize;
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as usize;
        // the file rounded up to a page, plus one more chunk of zeros.
        // do this so that if a chunk is less than CHUNK_SIZE from the final page,
        // it doesn't panic (the chunk would extend past the page otherwise).
        //
        // we map the file onto the start of the first map, so that reads past the end of the file do not fault.
        let map_len = len.next_multiple_of(page) + CHUNK_SIZE;

        let base = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                map_len,
                libc::PROT_READ,
                libc::MAP_PRIVATE | libc::MAP_ANON,
                -1,
                0,
            )
        };
        if base == libc::MAP_FAILED {
            return Err(std::io::Error::last_os_error());
        }

        if len > 0 {
            // replace the front of the reservation with the file.
            let mapped = unsafe {
                libc::mmap(
                    base,
                    len,
                    libc::PROT_READ,
                    libc::MAP_SHARED | libc::MAP_FIXED,
                    file.as_raw_fd(),
                    0,
                )
            };
            if mapped == libc::MAP_FAILED {
                let err = std::io::Error::last_os_error();
                unsafe { libc::munmap(base, map_len) };
                return Err(err);
            }
            // we're going to be iterating over the file sequentially, and assume
            // we will need the entire thing.
            unsafe { libc::madvise(base, len, libc::MADV_SEQUENTIAL | libc::MADV_WILLNEED) };
        }
        // make a slice now to avoid having to do this on every access
        let mmap = unsafe { std::slice::from_raw_parts(base as *const u8, len) };
        Ok(AlignedBuffer { map_len, start: 0, line_start: 0, mmap: mmap})
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

impl Drop for AlignedBuffer {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.mmap.as_ptr() as *mut libc::c_void, self.map_len) };
    }
}

#[cfg(test)]
mod buftests {
    use crate::aligned_buffer::AlignedBuffer;
    use crate::constants::CHUNK_SIZE;
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
