use crate::constants::CHUNK_SIZE;

pub(crate) mod mmap {
    use super::CHUNK_SIZE;
    use std::fs::File;
    use std::io;
    use std::ops::Deref;
    use std::os::fd::AsRawFd;

    pub(crate) fn page_size() -> usize {
        unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize }
    }

    pub struct MappedFile {
        base: *const u8,
        len: usize,
        // total size mapped, including zero buffer, for drop
        map_len: usize,
    }

    impl MappedFile {
        pub fn new(file: &File) -> io::Result<Self> {
            let len = file.metadata()?.len() as usize;
            // the file rounded up to a page, plus one more chunk of zeros.
            let map_len = len.next_multiple_of(page_size()) + CHUNK_SIZE;

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
                return Err(io::Error::last_os_error());
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
                    let err = io::Error::last_os_error();
                    unsafe { libc::munmap(base, map_len) };
                    return Err(err);
                }
                // we're going to be iterating over the file sequentially, and assume
                // we will need the entire thing.
                unsafe { libc::madvise(base, len, libc::MADV_SEQUENTIAL | libc::MADV_WILLNEED) };
            }
            Ok(Self { base: base as *const u8, len, map_len })
        }
    }

    impl Deref for MappedFile {
        type Target = [u8];

        #[inline(always)]
        fn deref(&self) -> &Self::Target {
            unsafe { std::slice::from_raw_parts(self.base, self.len) }
        }
    }

    impl Drop for MappedFile {
        fn drop(&mut self) {
            unsafe { libc::munmap(self.base as *mut libc::c_void, self.map_len) };
        }
    }
}
