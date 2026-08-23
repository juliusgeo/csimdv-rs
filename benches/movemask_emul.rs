use std::arch::aarch64::{uint16x8_t, uint8x16_t, uint8x16x4_t, uint8x8_t, vdupq_n_u8, vld4q_u8, vreinterpret_u64_u8, vreinterpretq_u16_u8, vshrn_n_u16, vsriq_n_u8};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode};
use std::fs;
use std::time::Duration;
mod common;
use std::hint::black_box;

use std::arch::aarch64::{vceqq_u8, vbslq_u8, vshrn_n_s16, vreinterpretq_s16_u8, vreinterpret_u64_s8, vget_lane_u64};
const ITERATIONS: usize = 1_000_000_000;
#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
fn movemask_bit_select() {
    unsafe {
        let bit_select_mask_1 =  unsafe { vdupq_n_u8(0x55) };
        let bit_select_mask_2 =  unsafe { vdupq_n_u8(0x33) };
        let to_bitmask = |input: uint8x16x4_t| -> u64 {
            // isolate 01010101 and 23232323
            let t0 = vbslq_u8(bit_select_mask_1, input.0, input.1); // 01010101...
            let t1 = vbslq_u8(bit_select_mask_1, input.2, input.3); // 23232323...
            let combined = vbslq_u8(bit_select_mask_2, t0, t1); // 01230123...
            let sum = vshrn_n_s16::<4>(vreinterpretq_s16_u8(combined));
            return vget_lane_u64::<0>(vreinterpret_u64_s8(sum));
        };
        let chunk = [0u8; 64];
        let mask = unsafe { vld4q_u8(chunk.as_ptr()) };
        for _ in 0..ITERATIONS {
            black_box(to_bitmask(mask));
        }
    }
}


#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
fn movemask_vshrn() {
    unsafe {
        let to_bitmask = |input: uint8x16x4_t| -> u64 {
            unsafe {
                let v0 = vsriq_n_u8::<1>(input.1, input.0);
                let v6 = vsriq_n_u8::<1>(input.3, input.2);
                let v6 = vsriq_n_u8::<2>(v6, v0);
                let v6 = vsriq_n_u8::<4>(v6, v6);

                let v6h: uint16x8_t = vreinterpretq_u16_u8(v6);
                let v0n: uint8x8_t = vshrn_n_u16::<4>(v6h);

                vget_lane_u64::<0>(vreinterpret_u64_u8(v0n))
            }
        };
        let chunk = [0u8; 64];
        let mask = unsafe { vld4q_u8(chunk.as_ptr()) };
        for _ in 0..ITERATIONS {
            black_box(to_bitmask(mask));
        }
    }
}

fn comparison_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("Movemask Emulation Comparison");
    group.sampling_mode(SamplingMode::Flat);
    group.bench_function("method 1 (bit select mask)", |c| c.iter(|| movemask_bit_select()));
    group.bench_function("method 1 (narrowing)", |c| c.iter(|| movemask_vshrn()));
    group.finish();
}

criterion_group!(name = benches;
                 config = Criterion::default().measurement_time(Duration::from_secs(10));
                 targets = comparison_benchmark);
criterion_main!(benches);