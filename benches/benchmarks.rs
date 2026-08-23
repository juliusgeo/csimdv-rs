use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode};
use std::fs;
use std::time::Duration;
mod common;

use common::{parse_file_csv, parse_file_simd_csv_zerocopy, parse_file_csimdv};

fn collect_paths(basepath: &str) -> Vec<String> {
    let paths = fs::read_dir(basepath)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    return paths
}
fn comparison_benchmark(c: &mut Criterion) {
    let paths = collect_paths("examples");
    let mut group = c.benchmark_group("CSV Parsing Comparison");
    group.sampling_mode(SamplingMode::Flat);
    for path in paths.iter() {
        let metadata = fs::metadata(path).unwrap();
        group.throughput(criterion::Throughput::Bytes(metadata.len()));
        group.bench_with_input(BenchmarkId::new("csv", path), path, |c, p| c.iter(|| parse_file_csv(p)));
        group.bench_with_input(BenchmarkId::new("simdcsv", path), path,|c, p| c.iter(|| parse_file_simd_csv_zerocopy(p)));
        group.bench_with_input(BenchmarkId::new("csimdv", path), path, |c, p| c.iter(|| parse_file_csimdv(p)));
    }
    group.finish();
}

criterion_group!(name = benches;
                 config = Criterion::default().measurement_time(Duration::from_secs(50));
                 targets = comparison_benchmark);
criterion_main!(benches);