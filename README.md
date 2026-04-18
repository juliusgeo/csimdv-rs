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
The bulk of the runtime of this parser is spent classifying the input characters 64 bytes at a time, using the table lookup approach from `simdjson`.
The target architecture plays a large role in how effective this approach is compared to `simd-csv`. 
I initially implemented this using `portable_simd`, but it results in suboptimal code generation,
especially on aarch64, where there is no equivalent to the `movemask` x86 instruction. I worked around that aspect by loading 
the data interleaved into NEON vectors, allowing the usage of some more efficient bitmask generation techniques.

The following benchmark results were all calculated using `criterion-rs` with a `flat` sampling mode with a sampling time of 100s.

### `aarch64 NEON` 

| File                                                  | `csimdv`     | `simd-csv`   | % Change |
|-------------------------------------------------------|--------------|--------------|----------|
| [EDW.TEST_CAL_DT.csv](examples%2FEDW.TEST_CAL_DT.csv) | 2.0818 GiB/s | 2.1836 GiB/s | -4.7     |
| [nfl.csv](examples%2Fnfl.csv)                         | 2.2262 GiB/s | 1.9017 GiB/s | 17.1     |
| customers-2000000.csv (not committable, too large)    | 2.3811 /s    | 1.7857 GiB/s | 33.3     |

Ran on an Apple M1 Max with 64GB of RAM.

### `x86_64 AVX-512`

| File                                                  | `csimdv`     | `simd-csv`   | % Change |
|-------------------------------------------------------|--------------|--------------|----------|
| [EDW.TEST_CAL_DT.csv](examples%2FEDW.TEST_CAL_DT.csv) | 2.7538 GiB/s | 1.9902 GiB/s | 38.37    |
| [nfl.csv](examples%2Fnfl.csv)                         | 2.5444 GiB/s | 1.9423 GiB/s | 31.00    |
| customers-2000000.csv (not committable, too large)    | 2.6522 GiB/s | 1.6383 GiB/s | 61.89    |

### `x86_64 AVX-2`

| File                                                  | `csimdv`     | `simd-csv`   | % Change |
|-------------------------------------------------------|--------------|--------------|----------|
| [EDW.TEST_CAL_DT.csv](examples%2FEDW.TEST_CAL_DT.csv) | 2.7521 GiB/s | 2.0566 GiB/s | 33.82    |
| [nfl.csv](examples%2Fnfl.csv)                         | 2.5316 GiB/s | 2.0226 GiB/s | 25.17    |
| customers-2000000.csv (not committable, too large)    | 2.6630 GiB/s | 1.7053 GiB/s | 56.16    |

Ran on an AMD Ryzen 7 9800x3d with 32GB of RAM, with `RUSTFLAGS="-C target-cpu=native -C target-feature=-avx512f"` for AVX2.