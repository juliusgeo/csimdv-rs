use criterion::{
    BenchmarkId, Criterion, SamplingMode, Throughput, criterion_group, criterion_main,
};
use std::hint::black_box;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::fs::File;
use std::time::Duration;

use csimdv::aligned_buffer::AlignedBuffer;
use csimdv::de::deserialize_record;
use csimdv::{ByteReader, Parser, default_dialect};

/// `customers-2000000.csv`
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // fields exist to be deserialized into, not read
struct Customer {
    #[serde(rename = "Index")]
    index: u64,
    #[serde(rename = "Customer Id")]
    customer_id: String,
    #[serde(rename = "First Name")]
    first_name: String,
    #[serde(rename = "Last Name")]
    last_name: String,
    #[serde(rename = "City")]
    city: String,
    #[serde(rename = "Country")]
    country: String,
}



/// `nfl.csv`
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // fields exist to be deserialized into, not read
struct Play {
    gameid: String,
    qtr: u64,
    off: String,
    def: String,
    description: String,
    offscore: u64,
    defscore: u64,
    season: u64,
}


fn byte_reader(path: &str) -> ByteReader {
    let file = File::open(path).unwrap();
    let parser = Parser::new(default_dialect(), AlignedBuffer::new(&file).unwrap());
    ByteReader::with_headers(parser)
}

/// `SerdeReader<D>` driven as a plain iterator.
fn csimdv_owned<D: DeserializeOwned>(path: &str) -> usize {
    let file = File::open(path).unwrap();
    let rdr = csimdv::SerdeReader::<D>::new(default_dialect(), &file);
    let mut rows = 0;
    for row in rdr {
        black_box(&row.unwrap());
        rows += 1;
    }
    rows
}

fn csv_owned<D: DeserializeOwned>(path: &str) -> usize {
    let file = File::open(path).unwrap();
    let mut rdr = csv::ReaderBuilder::new().has_headers(true).from_reader(file);
    let mut rows = 0;
    for result in rdr.deserialize::<D>() {
        black_box(&result.unwrap());
        rows += 1;
    }
    rows
}

macro_rules! bench_file {
    ($group:expr, $path:expr, $owned:ty) => {{
        let path: &str = $path;
        match std::fs::metadata(path) {
            Ok(md) => {
                $group.throughput(Throughput::Bytes(md.len()));

                $group.bench_with_input(
                    BenchmarkId::new("csimdv deserialize borrowed", path),
                    path,
                    |b, p| b.iter(|| csimdv_owned::<$owned>(p)),
                );
                $group.bench_with_input(
                    BenchmarkId::new("csv deserialize owned", path),
                    path,
                    |b, p| b.iter(|| csv_owned::<$owned>(p)),
                );
            }
            Err(_) => eprintln!("skipping {path}: not found"),
        }
    }};
}

fn deserialize_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("CSV Deserialization");
    group.sampling_mode(SamplingMode::Flat);

    bench_file!(
        group,
        "examples/nfl.csv",
        Play
    );
    bench_file!(
        group,
        "examples/customers-2000000.csv",
        Customer
    );

    group.finish();
}

criterion_group!(name = benches;
                 config = Criterion::default()
                     .measurement_time(Duration::from_secs(50));
                 targets = deserialize_benchmark);
criterion_main!(benches);
