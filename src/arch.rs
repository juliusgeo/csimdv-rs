#[cfg(all(target_arch = "x86_64", target_feature = "pclmulqdq"))]
pub(crate) mod prefix_xor {
    pub(crate) type ChunkSimd = core::arch::aarch64::uint8x16x4_t;
    pub fn clmul64(a: u64, b:u64) -> u64{
        unsafe {
            use core::arch::x86_64::*;
            let va = _mm_set_epi64x(0, a as i64);
            let vb = _mm_set_epi64x(0, b as i64);
            let r = _mm_cvtsi128_si64(_mm_clmulepi64_si128(va, vb, 0x00)) as u64;
            r as u64
        }
    }

}

#[cfg(all(target_arch = "aarch64", target_feature = "aes"))]
pub(crate) mod prefix_xor {
    pub fn clmul64(a: u64, b:u64) -> u64{
        unsafe {
            use core::arch::aarch64::*;
            let r = vmull_p64(a, b);
            r as u64
        }
    }

}



#[cfg(all(target_arch = "x86_64", target_feature = "avx512f"))]
pub(crate) mod simd {
    use core::arch::x86_64::*;
    pub(crate) type ChunkSimd = __m512i;

    pub fn simd_eq_bitmask(chunk: ChunkSimd,
                           delimiters: ChunkSimd,
                           quotes: ChunkSimd,
                           newlines: ChunkSimd,
                           returns: ChunkSimd) -> (u64, u64, u64, u64) {
        unsafe {
            unsafe fn lane_eq_bitmask(a: __m512i, b: __m512i) -> u64 {
                _mm512_cmpeq_epi8_mask(a, b)
            }
            (lane_eq_bitmask(chunk, delimiters), lane_eq_bitmask(chunk, quotes), lane_eq_bitmask(chunk, newlines), lane_eq_bitmask(chunk, returns))
        }
    }

    pub fn load_simd(a: *const u8) -> ChunkSimd {
        unsafe {
            use core::arch::x86_64::*;
            let chunk_ptr = a.as_ptr() as *const __m512i;
            _mm512_loadu_si512(chunk_ptr)
        }
    }
}
#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
pub(crate) mod simd {
    use core::arch::x86_64::*;

    pub(crate) type ChunkSimd = (__m256i, __m256i);
    pub fn simd_eq_bitmask(chunk: ChunkSimd,
                           delimiters: ChunkSimd,
                           quotes: ChunkSimd,
                           newlines: ChunkSimd,
                           returns: ChunkSimd) -> (u64, u64, u64, u64) {
        unsafe {
            unsafe fn lane_eq_bitmask(a: (__m256i, __m256i), b: (__m256i, __m256i)) -> u64 {
                let cmp1 = _mm256_movemask_epi8(_mm256_cmpeq_epi8(a.0, b.0)) as u32 as u64;
                let cmp2 = _mm256_movemask_epi8(_mm256_cmpeq_epi8(a.1, b.1)) as u32 as u64;
                (cmp1 | cmp2 << 32)
            }
            (lane_eq_bitmask(chunk, delimiters), lane_eq_bitmask(chunk, quotes), lane_eq_bitmask(chunk, newlines), lane_eq_bitmask(chunk, returns))

        }
    }

    pub fn load_simd(p: *const u8) -> ChunkSimd {
        unsafe {
            use core::arch::x86_64::*;
            let r0 = _mm256_loadu_si256(p as *const __m256i);
            let r1 = _mm256_loadu_si256(p.add(32) as *const __m256i);
            (r0, r1)
        }
    }
}
#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
pub(crate) mod simd {
    use core::arch::aarch64::*;

    pub const COMMA: u8 = 2;
    pub const NEWLINE: u8 = 4;
    pub const QUOTES: u8 = 8;

    pub const BYTE_TABLE: [u8; 64] = {
        let mut out = [0u8; 64];
        out[0x0A] = NEWLINE;
        out[0x0D] = NEWLINE;
        out[0x2C] = COMMA;
        out[0x22] = QUOTES;
        out
    };

    pub struct Classifier {
        bit_select_mask_1: uint8x16_t,
        bit_select_mask_2: uint8x16_t,
        byte_table: uint8x16x4_t,
        comma_splat: uint8x16_t,
        newline_splat: uint8x16_t,
        quote_splat: uint8x16_t,
    }
    impl Classifier {
        pub fn new() -> Self {
            Self {
                byte_table: unsafe { vld1q_u8_x4(&BYTE_TABLE as *const u8) },
                bit_select_mask_1: unsafe { vdupq_n_u8(0x55) },
                bit_select_mask_2: unsafe { vdupq_n_u8(0x33) },
                comma_splat: unsafe { vdupq_n_u8(COMMA) },
                newline_splat: unsafe { vdupq_n_u8(NEWLINE) },
                quote_splat: unsafe { vdupq_n_u8(QUOTES) },
            }
        }

        #[inline(always)]
        pub fn classify(&self, chunk: &[u8]) -> (u64, u64, u64) {
            unsafe {
                // load the chunk interleaved (this makes the movemask emulation easier at the end).
                let chunk = vld4q_u8(chunk.as_ptr());
                // 4 instrs
                // according to this table: https://dougallj.github.io/applecpu/firestorm-simd.html,
                // vqtbl4q is only twice the the latency of a single register vqtbl (4 vs 2), but it
                // halves the number of table lookups we need to do (for high and low nibbles), and removes
                // the need to and them. So in all, it should save us about 4 instructions.
                let classified = uint8x16x4_t(
                    vqtbl4q_u8(self.byte_table, chunk.0),
                    vqtbl4q_u8(self.byte_table, chunk.1),
                    vqtbl4q_u8(self.byte_table, chunk.2),
                    vqtbl4q_u8(self.byte_table, chunk.3),
                );

                let to_bitmask = |input: uint8x16x4_t, s: uint8x16_t| -> u64 {
                    // isolate 01010101 and 23232323
                    let t0 = vbslq_u8(self.bit_select_mask_1, vceqq_u8(input.0, s), vceqq_u8(input.1, s)); // 01010101...
                    let t1 = vbslq_u8(self.bit_select_mask_1, vceqq_u8(input.2, s), vceqq_u8(input.3, s)); // 23232323...
                    let combined = vbslq_u8(self.bit_select_mask_2, t0, t1); // 01230123...
                    let sum = vshrn_n_s16::<4>(vreinterpretq_s16_u8(combined));
                    return vget_lane_u64::<0>(vreinterpret_u64_s8(sum));
                };
                (to_bitmask(classified, self.comma_splat),
                        to_bitmask(classified, self.quote_splat),
                        to_bitmask(classified, self.newline_splat))
            }
        }
    }
}