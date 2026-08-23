use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::io::{BufWriter, Write};
use tempfile::NamedTempFile;
use rand::RngExt;

#[derive(Clone, Debug)]
pub struct CsvGenConfig {
    pub columns: usize,
    pub quoted_pct: f64,
    pub min_field_len: usize,
    pub max_field_len: usize,
    pub seed: u64,
}

impl Default for CsvGenConfig {
    fn default() -> Self {
        Self {
            columns: 8,
            quoted_pct: 0.2,
            min_field_len: 4,
            max_field_len: 16,
            seed: 0x5EED,
        }
    }
}

const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 ";

fn random_ascii(rng: &mut impl Rng, len: usize) -> String {
    (0..len)
        .map(|_| CHARSET[rng.random_range(0..CHARSET.len())] as char)
        .collect()
}

fn make_field(rng: &mut impl Rng, quoted: bool, min_len: usize, max_len: usize) -> String {
    let len = rng.random_range(min_len..=max_len.max(min_len));
    let mut s = random_ascii(rng, len);

    if quoted {
        if rng.random_bool(0.3) && !s.is_empty() {
            let pos = rng.random_range(0..=s.len());
            s.insert(pos, ',');
        }
        if rng.random_bool(0.15) && !s.is_empty() {
            let pos = rng.random_range(0..=s.len());
            s.insert(pos, '"');
        }
        let escaped = s.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        s
    }
}

fn generate_row(rng: &mut impl Rng, config: &CsvGenConfig) -> String {
    (0..config.columns)
        .map(|_| {
            let quoted = rng.random_bool(config.quoted_pct.clamp(0.0, 1.0));
            make_field(rng, quoted, config.min_field_len, config.max_field_len)
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn header_line(columns: usize) -> String {
    (0..columns)
        .map(|i| format!("col{i}"))
        .collect::<Vec<_>>()
        .join(",")
        + "\n"
}

pub fn generate_csv_of_size(size_mb: f64, config: &CsvGenConfig) -> NamedTempFile {
    let target_bytes = ((size_mb * 1024.0 * 1024.0).max(1.0)) as usize;
    let mut rng = StdRng::seed_from_u64(config.seed);

    let file = NamedTempFile::new().expect("failed to create temp file for synthetic csv");
    {
        let mut writer = BufWriter::new(file.as_file());

        let header = header_line(config.columns);
        writer.write_all(header.as_bytes()).unwrap();
        let mut written = header.len();

        while written < target_bytes {
            let line = generate_row(&mut rng, config) + "\n";
            writer.write_all(line.as_bytes()).unwrap();
            written += line.len();
        }
        writer.flush().unwrap();
    }
    file
}

pub fn generate_csv_with_rows(rows: usize, config: &CsvGenConfig) -> NamedTempFile {
    let mut rng = StdRng::seed_from_u64(config.seed);

    let file = NamedTempFile::new().expect("failed to create temp file for synthetic csv");
    {
        let mut writer = BufWriter::new(file.as_file());

        writer.write_all(header_line(config.columns).as_bytes()).unwrap();
        for _ in 0..rows {
            let line = generate_row(&mut rng, config) + "\n";
            writer.write_all(line.as_bytes()).unwrap();
        }
        writer.flush().unwrap();
    }
    file
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_target_is_roughly_respected() {
        let config = CsvGenConfig {
            columns: 6,
            quoted_pct: 0.5,
            ..CsvGenConfig::default()
        };
        let file = generate_csv_of_size(1.0, &config);
        let len = std::fs::metadata(file.path()).unwrap().len();
        let target = 1024 * 1024;
        // Should be at or just past the target, never way under.
        assert!(len >= target as u64);
        assert!(len < target as u64 + 4096);
    }

    #[test]
    fn row_count_is_exact() {
        let config = CsvGenConfig::default();
        let file = generate_csv_with_rows(500, &config);
        let content = std::fs::read_to_string(file.path()).unwrap();
        // header + 500 rows
        assert_eq!(content.lines().count(), 501);
    }

    #[test]
    fn same_seed_is_deterministic() {
        let config = CsvGenConfig {
            seed: 7,
            ..CsvGenConfig::default()
        };
        let a = generate_csv_with_rows(50, &config);
        let b = generate_csv_with_rows(50, &config);
        assert_eq!(
            std::fs::read_to_string(a.path()).unwrap(),
            std::fs::read_to_string(b.path()).unwrap()
        );
    }
}