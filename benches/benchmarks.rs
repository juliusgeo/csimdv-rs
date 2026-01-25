#![feature(test)]
use simd_csv::ZeroCopyReader;
use csimdv::default_dialect;
use csimdv::Parser;
use std::fs::File;
use lender::Lender;
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use csimdv::aligned_buffer::AlignedBuffer;
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
        while let Some(field) = record.next() {
            let _ = field.len();
        }
    }
}

fn comparison_benchmark(c: &mut Criterion) {
    let path = "examples/customers-2000000.csv";
    c.bench_function("parse_file_simd_csv_zerocopy", |c| c.iter(|| parse_file_simd_csv_zerocopy(black_box(path))));
    c.bench_function("parse_file_csimdv", |c| c.iter(|| parse_file_csimdv(black_box(path))));
}

criterion_group!(benches, comparison_benchmark);
criterion_main!(benches);