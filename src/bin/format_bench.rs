use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::Path;

use serde_json::Value;

struct Record {
    group: String,
    function: String,
    value: String,
    mean_ns: f64,
    median_ns: f64,
    typical_ns: f64,
    throughput: Option<(f64, String)>, // (per_iteration, unit)
}

fn parse_estimate(v: &Value, key: &str) -> Option<f64> {
    v.get(key)?.get("estimate")?.as_f64()
}

fn format_time(ns: f64, precision: usize) -> String {
    if ns.is_nan() {
        return "-".to_string();
    }
    let (val, unit) = if ns < 1_000.0 {
        (ns, "ns")
    } else if ns < 1_000_000.0 {
        (ns / 1_000.0, "µs")
    } else if ns < 1_000_000_000.0 {
        (ns / 1_000_000.0, "ms")
    } else {
        (ns / 1_000_000_000.0, "s")
    };
    format!("{:.*} {}", precision, val, unit)
}

fn format_throughput(per_iteration: f64, unit: &str, time_ns: f64, precision: usize) -> String {
    if unit != "bytes" {
        // Criterion also supports Elements throughput; just show raw rate.
        let per_sec = per_iteration / (time_ns / 1_000_000_000.0);
        return format!("{:.*} {}/s", precision, per_sec, unit);
    }
    let bytes_per_sec = per_iteration / (time_ns / 1_000_000_000.0);
    let units = ["B/s", "KiB/s", "MiB/s", "GiB/s", "TiB/s"];
    let mut v = bytes_per_sec;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{:.*} {}", precision, v, units[i])
}

fn get_metric_ns(r: &Record, metric: &str) -> f64 {
    match metric {
        "median" => r.median_ns,
        "typical" => r.typical_ns,
        _ => r.mean_ns,
    }
}

fn print_table(
    rows: &[String],
    cols: &[String],
    row_header: &str,
    metric: &str,
    precision: usize,
    cells: &HashMap<(String, String), &Record>,
    throughput_mode: bool,
) {
    let mut header = format!("| {} ", row_header);
    for c in cols {
        header.push_str(&format!("| {} ", c));
    }
    header.push('|');
    println!("{}", header);

    let mut sep = String::from("|---");
    for _ in cols {
        sep.push_str("|---");
    }
    sep.push('|');
    println!("{}", sep);

    for r in rows {
        let mut line = format!("| {} ", r);
        for c in cols {
            let text = match cells.get(&(r.clone(), c.clone())) {
                Some(rec) => {
                    if throughput_mode {
                        match &rec.throughput {
                            Some((per_iter, unit)) => {
                                let ns = get_metric_ns(rec, metric);
                                format_throughput(*per_iter, unit, ns, precision)
                            }
                            None => "-".to_string(),
                        }
                    } else {
                        format_time(get_metric_ns(rec, metric), precision)
                    }
                }
                None => "-".to_string(),
            };
            line.push_str(&format!("| {} ", text));
        }
        line.push('|');
        println!("{}", line);
    }
}

fn print_help() {
    println!(
        "criterion_md - format Criterion JSON-lines benchmark output as a markdown table

USAGE:
    criterion_md [OPTIONS] [INPUT_FILE]

    If INPUT_FILE is omitted, reads from stdin.

    Produce INPUT_FILE with cargo-criterion's json output, e.g.:
        cargo criterion --message-format=json > bench.jsonl
    Any non-JSON lines and any messages other than benchmark-complete /
    group-complete are ignored, so it's fine to pass in a raw log.

OPTIONS:
    --metric <mean|median|typical>   Which statistic to base values on (default: mean)
    --time                           Print the time table (µs/ms/s per cell)
    --throughput                     Print the throughput table (KiB/s, MiB/s, ...)
                                      (only for benchmarks with Throughput set)
                                      Default when neither --time nor --throughput
                                      is given: throughput only.
                                      Pass both to get both tables.
    --precision <N>                  Decimal places to show (default: 2)
    -h, --help                       Show this help"
    );
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut input_path: Option<String> = None;
    let mut metric = "mean".to_string();
    let mut show_time = false;
    let mut show_throughput = false;
    let mut precision = 2usize;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--metric" => {
                i += 1;
                metric = args.get(i).cloned().unwrap_or_else(|| "mean".into());
            }
            "--time" => show_time = true,
            "--throughput" => show_throughput = true,
            "--precision" => {
                i += 1;
                precision = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(2);
            }
            "-h" | "--help" => {
                print_help();
                return;
            }
            other => input_path = Some(other.to_string()),
        }
        i += 1;
    }

    // Default: show throughput only. --time adds/switches to the time table.
    // Pass both --time and --throughput to get both tables.
    if !show_time && !show_throughput {
        show_throughput = true;
    }

    let input = match &input_path {
        Some(path) => fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("Failed to read {}: {}", path, e);
            std::process::exit(1);
        }),
        None => {
            let mut s = String::new();
            io::stdin().read_to_string(&mut s).unwrap_or_else(|e| {
                eprintln!("Failed to read stdin: {}", e);
                std::process::exit(1);
            });
            s
        }
    };

    let mut records: Vec<Record> = Vec::new();

    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("reason").and_then(|r| r.as_str()) != Some("benchmark-complete") {
            continue;
        }
        let id = match v.get("id").and_then(|i| i.as_str()) {
            Some(id) => id.to_string(),
            None => continue,
        };
        // "<group>/<function>/<parameter...>" - parameter may itself contain '/'.
        let parts: Vec<&str> = id.splitn(3, '/').collect();
        if parts.len() < 3 {
            continue;
        }
        let group = parts[0].to_string();
        let function = parts[1].to_string();
        let value = parts[2].to_string();

        let mean_ns = parse_estimate(&v, "mean").unwrap_or(f64::NAN);
        let median_ns = parse_estimate(&v, "median").unwrap_or(f64::NAN);
        let typical_ns = parse_estimate(&v, "typical").unwrap_or(mean_ns);

        let throughput = v
            .get("throughput")
            .and_then(|t| t.as_array())
            .and_then(|arr| arr.first())
            .and_then(|t| {
                let per_iter = t.get("per_iteration")?.as_f64()?;
                let unit = t.get("unit")?.as_str()?.to_string();
                Some((per_iter, unit))
            });

        records.push(Record {
            group,
            function,
            value,
            mean_ns,
            median_ns,
            typical_ns,
            throughput,
        });
    }

    if records.is_empty() {
        eprintln!("No benchmark-complete records found in input.");
        std::process::exit(1);
    }

    let mut groups: Vec<String> = Vec::new();
    let mut by_group: HashMap<String, Vec<&Record>> = HashMap::new();
    for r in &records {
        if !groups.contains(&r.group) {
            groups.push(r.group.clone());
        }
        by_group.entry(r.group.clone()).or_default().push(r);
    }
    let multi_group = groups.len() > 1;

    for group in &groups {
        let recs = &by_group[group];

        let mut row_order: Vec<String> = Vec::new();
        let mut col_order: Vec<String> = Vec::new();
        let mut cells: HashMap<(String, String), &Record> = HashMap::new();

        for r in recs.iter() {
            if !row_order.contains(&r.function) {
                row_order.push(r.function.clone());
            }
            let basename = Path::new(&r.value)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| r.value.clone());
            if !col_order.contains(&basename) {
                col_order.push(basename.clone());
            }
            cells.insert((r.function.clone(), basename), r);
        }

        if multi_group {
            println!("### {}\n", group);
        }

        if show_time {
            print_table(&row_order, &col_order, "Library", &metric, precision, &cells, false);
            if show_throughput {
                println!();
            }
        }

        if show_throughput {
            print_table(&row_order, &col_order, "Library", &metric, precision, &cells, true);
        }
        println!();
    }
}