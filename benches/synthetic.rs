use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode, Throughput};
use std::fs;
use std::time::Duration;
mod common;

use common::config::SyntheticConfig;
use common::csv_gen::{generate_csv_of_size, CsvGenConfig};
use common::{parse_file_csimdv, parse_file_csv, parse_file_simd_csv_zerocopy};

fn synthetic_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("Synthetic Quoted-Field Sweep");
    group.sampling_mode(SamplingMode::Flat);

    let cfg = SyntheticConfig::from_env();
    eprintln!(
        "synthetic bench config: size_mb={} columns={} quoted_steps={:?} seed={}",
        cfg.size_mb, cfg.columns, cfg.quoted_steps, cfg.seed
    );

    let generated: Vec<(u32, tempfile::NamedTempFile, std::path::PathBuf)> = cfg
        .quoted_steps
        .iter()
        .map(|&pct| {
            let config = CsvGenConfig {
                columns: cfg.columns,
                quoted_pct: pct as f64 / 100.0,
                seed: cfg.seed,
                ..CsvGenConfig::default()
            };
            let file = generate_csv_of_size(cfg.size_mb, &config);
            let path = file.path().to_path_buf();
            (pct, file, path)
        })
        .collect();

    for (pct, _file, path) in &generated {
        let metadata = fs::metadata(path).unwrap();
        group.throughput(Throughput::Bytes(metadata.len()));

        let label = format!("{pct}pct_quoted");
        group.bench_with_input(BenchmarkId::new("csv", &label), path, |b, p| {
            b.iter(|| parse_file_csv(p))
        });
        group.bench_with_input(BenchmarkId::new("simdcsv", &label), path, |b, p| {
            b.iter(|| parse_file_simd_csv_zerocopy(p))
        });
        group.bench_with_input(BenchmarkId::new("csimdv", &label), path, |b, p| {
            b.iter(|| parse_file_csimdv(p))
        });
    }

    group.finish();
}

criterion_group!(name = benches;
                 config = Criterion::default().measurement_time(Duration::from_secs(20));
                 targets = synthetic_benchmark);
criterion_main!(benches);