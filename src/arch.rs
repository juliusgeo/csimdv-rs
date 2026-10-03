#[cfg(all(target_arch = "x86_64", target_feature = "pclmulqdq"))]
pub(crate) mod prefix_xor {
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

    pub struct Classifier {
        comma_splat: __m512i,
        newline_splat: __m512i,
        quote_splat: __m512i,
        return_splat: __m512i
    }
    impl Classifier {
        pub fn new() -> Self {
            Self {
                comma_splat: unsafe { _mm512_set1_epi8(',' as i8) },
                newline_splat: unsafe { _mm512_set1_epi8('\n' as i8) },
                return_splat: unsafe { _mm512_set1_epi8('\r' as i8) },
                quote_splat: unsafe { _mm512_set1_epi8('\"' as i8) },
            }
        }

        #[inline(always)]
        pub fn classify(&self, chunk: &[u8]) -> (u64, u64, u64) {
            unsafe {
                let chunk = _mm512_loadu_si512(chunk.as_ptr() as *const __m512i);
                (_mm512_cmpeq_epu8_mask(chunk, self.comma_splat),
                 _mm512_cmpeq_epu8_mask(chunk, self.quote_splat),
                 _mm512_cmpeq_epu8_mask(chunk, self.newline_splat) | _mm512_cmpeq_epu8_mask(chunk, self.return_splat))
            }
        }
    }
}

#[cfg(all(target_arch = "x86_64", target_feature = "avx2", not(target_feature="avx512f")))]
pub(crate) mod simd {
    use core::arch::x86_64::*;

    pub struct Classifier {
        comma_splat: (__m256i, __m256i),
        newline_splat: (__m256i, __m256i),
        quote_splat: (__m256i, __m256i),
        return_splat: (__m256i, __m256i),
    }

    pub fn load_simd(p: *const u8) -> (__m256i, __m256i) {
        unsafe {
            use core::arch::x86_64::*;
            let r0 = _mm256_loadu_si256(p as *const __m256i);
            let r1 = _mm256_loadu_si256(p.add(32) as *const __m256i);
            (r0, r1)
        }
    }

    impl Classifier {
        #[inline(always)]
        pub fn new() -> Self {
            unsafe {
                Self {
                    comma_splat: load_simd([b','; 64].as_ptr()),
                    newline_splat: load_simd([b'\n'; 64].as_ptr()),
                    return_splat: load_simd([b'\r'; 64].as_ptr()),
                    quote_splat: load_simd([b'\"'; 64].as_ptr()),
                }
            }
        }

        #[inline(always)]
        pub fn classify(&self, chunk: &[u8]) -> (u64, u64, u64) {
            unsafe {
                fn lane_eq_bitmask(a: (__m256i, __m256i), b: (__m256i, __m256i)) -> u64 {
                    unsafe {
                        let cmp1 = _mm256_movemask_epi8(_mm256_cmpeq_epi8(a.0, b.0)) as u32 as u64;
                        let cmp2 = _mm256_movemask_epi8(_mm256_cmpeq_epi8(a.1, b.1)) as u32 as u64;
                        (cmp1 | cmp2 << 32)
                    }
                }
                let chunk = load_simd(chunk.as_ptr());
                (lane_eq_bitmask(chunk, self.comma_splat), lane_eq_bitmask(chunk, self.quote_splat), lane_eq_bitmask(chunk, self.newline_splat) | lane_eq_bitmask(chunk, self.return_splat))
            }
        }
    }
}


#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
pub(crate) mod simd {
    use core::arch::aarch64::*;
    use crate::Dialect;

    pub const NEWLINE: u8 = '\n' as u8;
    pub const RETURN: u8 = '\r' as u8;

    pub struct Classifier {
        bit_select_mask_1: uint8x16_t,
        bit_select_mask_2: uint8x16_t,
        comma_splat: uint8x16_t,
        quote_splat: uint8x16_t,
        newline_table: uint8x16_t,
    }
    impl Classifier {
        pub fn new(dialect: Dialect) -> Self {
            // 0xff at '\n' and '\r'. Both are below 16, so `vqtbl1q_u8` can use
            // each input byte as its own index: every other byte either hits a
            // zero entry or is out of range, which also gives zero.
            let mut newline_table = [0u8; 16];
            newline_table[NEWLINE as usize] = 0xff;
            newline_table[RETURN as usize] = 0xff;
            Self {
                bit_select_mask_1: unsafe { vdupq_n_u8(0x55) },
                bit_select_mask_2: unsafe { vdupq_n_u8(0x33) },
                comma_splat: unsafe { vdupq_n_u8(dialect.delimiter as u8) },
                quote_splat: unsafe { vdupq_n_u8(dialect.quotechar as u8) },
                newline_table: unsafe { vld1q_u8(newline_table.as_ptr()) },
            }
        }

        #[inline(always)]
        pub fn classify(&self, chunk: &[u8]) -> (u64, u64, u64) {
            unsafe {
                // load the chunk interleaved (this makes the movemask emulation easier at the end).
                let chunk = vld4q_u8(chunk.as_ptr());
                let to_bitmask = |input: uint8x16x4_t| -> u64 {
                    // isolate 01010101 and 23232323
                    let t0 = vbslq_u8(self.bit_select_mask_1, input.0, input.1); // 01010101...
                    let t1 = vbslq_u8(self.bit_select_mask_1, input.2, input.3); // 23232323...
                    let combined = vbslq_u8(self.bit_select_mask_2, t0, t1); // 01230123...
                    let sum = vshrn_n_s16::<4>(vreinterpretq_s16_u8(combined));
                    return vget_lane_u64::<0>(vreinterpret_u64_s8(sum));
                };
                let comma_equal = uint8x16x4_t(
                    vceqq_u8(chunk.0, self.comma_splat),
                    vceqq_u8(chunk.1, self.comma_splat),
                    vceqq_u8(chunk.2, self.comma_splat),
                    vceqq_u8(chunk.3, self.comma_splat),
                );
                let quote_equal = uint8x16x4_t(
                    vceqq_u8(chunk.0, self.quote_splat),
                    vceqq_u8(chunk.1, self.quote_splat),
                    vceqq_u8(chunk.2, self.quote_splat),
                    vceqq_u8(chunk.3, self.quote_splat),
                );
                // One table lookup per vector instead of two compares and an or.
                let newline_equal = uint8x16x4_t(
                    vqtbl1q_u8(self.newline_table, chunk.0),
                    vqtbl1q_u8(self.newline_table, chunk.1),
                    vqtbl1q_u8(self.newline_table, chunk.2),
                    vqtbl1q_u8(self.newline_table, chunk.3),
                );
                (
                    to_bitmask(comma_equal),
                    to_bitmask(quote_equal),
                    to_bitmask(newline_equal)
                )
            }
        }
    }
}