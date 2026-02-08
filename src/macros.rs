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