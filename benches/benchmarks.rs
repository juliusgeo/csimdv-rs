#![feature(test)]
use simd_csv::ZeroCopyReader;
use csimdv::default_dialect;
use csimdv::Parser;
use std::fs::File;
use lender::Lender;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use csimdv::aligned_buffer::AlignedBuffer;
use std::fs;

fn parse_file_simd_csv_zerocopy(path: &str){
    let file = File::open(path).unwrap();

    let mut reader = ZeroCopyReader::from_reader(file);
    while let Some(record) = reader.read_byte_record().unwrap() {
        for field in record.iter() {
            let _ = field.len();
        }
    }
}
fn parse_file_csimdv(path: &str){
    let file = File::open(path).unwrap();
    let mut p = Parser::new(default_dialect(), AlignedBuffer::new(file));
    while let Some(mut record) = p.read_line() {
        for field in record.iter() {
            let _ = field.len();
        }
    }
}
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
    for path in paths.iter() {
        let metadata = fs::metadata(path).unwrap();
        group.throughput(criterion::Throughput::Bytes(metadata.len()));
        group.bench_with_input(BenchmarkId::new("parse_file_simd_csv_zerocopy", path), path,|c, p| c.iter(|| parse_file_simd_csv_zerocopy(p)));
        group.bench_with_input(BenchmarkId::new("parse_file_csimdv", path), path, |c, p| c.iter(|| parse_file_csimdv(p)));
    }
    group.finish();
}

criterion_group!(benches, comparison_benchmark);
criterion_main!(benches);