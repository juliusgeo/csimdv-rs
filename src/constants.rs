pub(crate) const CHUNK_SIZE: usize = 64;
/// Read buffer size for the non-mmap AlignedBuffer (Windows). 256K beat 64K/1M/4M on
/// both small and large files: big enough to amortise read calls, small enough to
/// stay in cache and not pay page faults allocating a fresh buffer per file.
pub(crate) const BUFFER_SIZE: usize = 256 << 10;
