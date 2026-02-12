
#[cfg(all(target_arch = "x86_64", target_feature = "avx512f"))]
pub(crate) type ChunkSimd = core::arch::x86_64::__m512i;
#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
pub(crate) type ChunkSimd = (core::arch::x86_64::__m256i, core::arch::x86_64::__m256i);

#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
pub(crate) type ChunkSimd = core::arch::aarch64::uint8x16x4_t;


#[macro_export]
macro_rules! clmul64 {
    ($a:expr, $b:expr) => {{
        #[cfg(all(target_arch = "x86_64", target_feature = "pclmulqdq"))]
        unsafe {
            use core::arch::x86_64::*;
            let va = _mm_set_epi64x(0, $a as i64);
            let vb = _mm_set_epi64x(0, $b as i64);
            let r = _mm_cvtsi128_si64(_mm_clmulepi64_si128(va, vb, 0x00)) as u64;
            r
        }

        #[cfg(all(target_arch = "aarch64", target_feature = "aes"))]
        unsafe {
            use core::arch::aarch64::*;
            let r = vmull_p64($a, $b);
            r
        }

        #[cfg(not(any(
            all(target_arch = "x86_64", target_feature = "pclmulqdq"),
            all(target_arch = "aarch64", target_feature = "aes")
        )))]
        compile_error!("CLMUL not supported on this architecture");
    }};
}

#[macro_export]
macro_rules! simd_eq_bitmask {
    ($chunk:expr, $a:expr, $b:expr, $c:expr, $d:expr) => {{
        #[cfg(all(target_arch = "x86_64", target_feature = "avx512f"))]
        unsafe {
            use core::arch::x86_64::*;
            unsafe fn check_bytes_eq_avx512(a: __m512i, b: __m512i) -> u64 {
                _mm512_cmpeq_epi8_mask(a, b)
            }
            // dbg!(check_bytes_eq_avx512(chunk, a));
            (check_bytes_eq_avx512($chunk, $a), check_bytes_eq_avx512($chunk, $b), check_bytes_eq_avx512($chunk, $c), check_bytes_eq_avx512($chunk, $d))
        }

        #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
        unsafe {
            use core::arch::x86_64::*;
            unsafe fn check_bytes_eq_avx2(a: (__m256i, __m256i), b: (__m256i, __m256i)) -> u64 {
                let cmp1 = _mm256_movemask_epi8(_mm256_cmpeq_epi8(a.0, b.0)) as u32 as u64;
                let cmp2 = _mm256_movemask_epi8(_mm256_cmpeq_epi8(a.1, b.1)) as u32 as u64;
                (cmp1 | cmp2 << 32)
            }
            (check_bytes_eq_avx2($chunk, $a), check_bytes_eq_avx2($chunk, $b), check_bytes_eq_avx2($chunk, $c), check_bytes_eq_avx2($chunk, $d))
        }

        #[cfg(all(target_arch = "aarch64"))]
        unsafe {
            use core::arch::aarch64::*;

            // trick from here: https://validark.dev/posts/interleaved-vectors-on-arm/
            pub unsafe fn check_bytes_eq_neon(
                a: uint8x16x4_t,
                b: uint8x16x4_t,
            ) -> u64 {
                // cmeq (vector-vector)
                unsafe {

                    let cmp1 = vceqq_u8(a.2, b.2);
                    let cmp2 = vceqq_u8(a.3, b.3);
                    let cmp3 = vceqq_u8(a.0, b.0);
                    let cmp4 = vceqq_u8(a.1, b.1);

                    let v0 = vsriq_n_u8::<1>(cmp4, cmp3);
                    let v6 = vsriq_n_u8::<1>(cmp2, cmp1);
                    let v6 = vsriq_n_u8::<2>(v6, v0);
                    let v6 = vsriq_n_u8::<4>(v6, v6);

                    let v6h: uint16x8_t = vreinterpretq_u16_u8(v6);
                    let v0n: uint8x8_t = vshrn_n_u16::<4>(v6h);

                    vget_lane_u64::<0>(vreinterpret_u64_u8(v0n))
                }
            }
            (check_bytes_eq_neon($chunk, $a), check_bytes_eq_neon($chunk, $b), check_bytes_eq_neon($chunk, $c), check_bytes_eq_neon($chunk, $d))
        }

        #[cfg(not(any(
            all(target_arch = "x86_64", any(target_feature = "avx512f", target_feature="avx2")),
            all(target_arch = "aarch64", target_feature = "neon")
        )))]
        compile_error!("simd intrinsics not supported on this architecture");
    }};
}

#[macro_export]
macro_rules! load_simd {
    ($chunk:expr) => {{
        #[cfg(all(target_arch = "x86_64", target_feature = "avx512f"))]
        unsafe {
            use core::arch::x86_64::*;
            let chunk_ptr = $chunk.as_ptr() as *const __m512i;
            _mm512_loadu_si512(chunk_ptr)
        }

        #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
        unsafe {
            use core::arch::x86_64::*;
            let p = $chunk.as_ptr();
            let r0 = _mm256_loadu_si256(p as *const __m256i);
            let r1 = _mm256_loadu_si256(p.add(32) as *const __m256i);
            (r0, r1)
        }

        #[cfg(all(target_arch = "aarch64"))]
        unsafe {
            use core::arch::aarch64::*;
            vld4q_u8($chunk.as_ptr())

        }

        #[cfg(not(any(
            all(target_arch = "x86_64", any(target_feature = "avx512f", target_feature="avx2")),
            all(target_arch = "aarch64", target_feature = "neon")
        )))]
        compile_error!("simd intrinsics not supported on this architecture");
    }};
}