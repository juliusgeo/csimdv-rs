csimdv
======

An alternate approach to SIMD CSV parsing, heavily inspired by: https://github.com/medialab/simd-csv

Differences
----------

`simd-csv` is a fantastic library, however, as noted in the README, does *not* use the ["pclmulqdq"](https://branchfree.org/2019/03/06/code-fragment-finding-quote-pairs-with-carry-less-multiply-pclmulqdq/) trick that
many other SIMD based parsers do (most notably `simdjson`). To be fair, this trick does not work on all targets, which is
the stated reason that `simd-csv` does not use it. However, I wanted to see if I could make a version of `simd-csv` that 
did use this trick, and see how much of a performance boost it would give.

Similarities
----------
To make the comparison as fair as possible, I use an API which is very similar to `simd-csv`'s `ZeroCopyReader`, which does
not perform any validation or escaping of the raw CSV data. Thus, the comparison in speed should give a good sense of the 
speed of the parsing itself, irrespective of validation/string escaping/iterator overhead.
```rust
let file = File::open(path).unwrap();
let mut p = Parser::new(default_dialect(), AlignedBuffer::new(file));
while let Some(mut record) = p.read_line() {
    for field in record.iter() {
        let _ = field.len();
    }
}
```

Performance
----------

On AArch64, the table lookup approach used by `simdjson` is used because it saves 1 extra comparison between the data
and the return character, and the comparisons are quite slow. On x86, just directly comparing the input data and the 4
characters of interests is faster. I initially implemented this using `portable_simd`, but it results in suboptimal code generation,
especially on aarch64, where there is no equivalent to the `movemask` x86 instruction. I worked around that aspect by loading 
the data interleaved into NEON vectors, allowing the usage of some more efficient bitmask generation techniques.
The input file is memory mapped, which along with `MADV_SEQUENTIAL` and `MADV_WILLNEED` allows very fast I/O. 

The following benchmark results were all calculated using `criterion-rs` with a `flat` sampling mode with a sampling time of 50s (excluding a 3 second warmup).

### `aarch64 NEON` 

| Library | nfl.csv      | customers-2000000.csv | EDW.TEST_CAL_DT.csv |
|---------|--------------|-----------------------|---------------------|
| csv     | 645.82 MiB/s | 588.49 MiB/s          | 805.70 MiB/s        |
| simdcsv | 1.88 GiB/s   | 1.81 GiB/s            | 2.20 GiB/s          |
| csimdv  | 3.53 GiB/s   | 3.28 GiB/s            | 3.27 GiB/s          |

Ran on an Apple M1 Max with 64GB of RAM.

### `x86_64 AVX-512`

| Library | customers-2000000.csv | EDW.TEST_CAL_DT.csv | nfl.csv      |
|---------|-----------------------|---------------------|--------------|
| csv     | 563.29 MiB/s          | 774.34 MiB/s        | 601.27 MiB/s |
| simdcsv | 1.67 GiB/s            | 2.06 GiB/s          | 1.95 GiB/s   |
| csimdv  | 3.86 GiB/s            | 3.85 GiB/s          | 4.35 GiB/s   |

### `x86_64 AVX-2`

| Library | customers-2000000.csv | EDW.TEST_CAL_DT.csv | nfl.csv      |
|---------|-----------------------|---------------------|--------------|
| csv     | 584.94 MiB/s          | 951.76 MiB/s        | 604.34 MiB/s |
| simdcsv | 1.69 GiB/s            | 2.06 GiB/s          | 2.06 GiB/s   |
| csimdv  | 3.77 GiB/s            | 3.88 GiB/s          | 4.33 GiB/s   |


Ran on an AMD Ryzen 7 9800x3d with 32GB of RAM, with `RUSTFLAGS="-C target-cpu=native -C target-feature=-avx512f"` for AVX2.