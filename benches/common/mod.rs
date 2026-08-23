pub mod csv_gen;
mod config;

use simd_csv::ZeroCopyReader;
use csimdv::default_dialect;
use csimdv::Parser;
use std::fs::File;
use csimdv::aligned_buffer::AlignedBuffer;
use csv::Reader;

pub fn parse_file_csv(path: &str) {
    let file = File::open(path).unwrap();
    let mut rdr = Reader::from_reader(file);
    for result in rdr.records() {
        let _ = result.unwrap();
    }
}
pub fn parse_file_simd_csv_zerocopy(path: &str){
    let file = File::open(path).unwrap();

    let mut reader = ZeroCopyReader::from_reader(file);
    while let Some(record) = reader.read_byte_record().unwrap() {
        for field in record.iter() {
            let _ = field.len();
        }
    }
}
pub fn parse_file_csimdv(path: &str){
    let file = File::open(path).unwrap();
    let mut p = Parser::new(default_dialect(), AlignedBuffer::new(&file).unwrap());
    while let Some(mut record) = p.read_line() {
        for field in record.iter() {
            let _ = field.len();
        }
    }
}