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

The following benchmark results were all calculated using `criterion-rs` with a `flat` sampling mode with a sampling time of 100s.

### `aarch64 NEON` 

| File                                                  | `csimdv`     | `simd-csv`   | % Change |
|-------------------------------------------------------|--------------|--------------|----------|
| [EDW.TEST_CAL_DT.csv](examples%2FEDW.TEST_CAL_DT.csv) | 2.0234 GiB/s | 2.1657 GiB/s | -6.5     |
| [nfl.csv](examples%2Fnfl.csv)                         | 2.4118 GiB/s | 1.8498 GiB/s | 30.4     |
| customers-2000000.csv (not committable, too large)    | 2.4165 GiB/s | 1.7753 GiB/s | 36.1     |

Ran on an Apple M1 Max with 64GB of RAM.

### `x86_64 AVX-512`

| File                                                  | `csimdv`     | `simd-csv`   | % Change |
|-------------------------------------------------------|--------------|--------------|----------|
| [EDW.TEST_CAL_DT.csv](examples%2FEDW.TEST_CAL_DT.csv) | 1.6645 GiB/s | 1.9766 GiB/s | -15.8    |
| [nfl.csv](examples%2Fnfl.csv)                         | 2.5073 GiB/s | 2.0066 GiB/s | 24.9     |
| customers-2000000.csv (not committable, too large)    | 3.6405 GiB/s | 1.6402 GiB/s | 121.9    |

### `x86_64 AVX-2`

| File                                                  | `csimdv`     | `simd-csv`   | % Change |
|-------------------------------------------------------|--------------|--------------|----------|
| [EDW.TEST_CAL_DT.csv](examples%2FEDW.TEST_CAL_DT.csv) | 1.7015 GiB/s | 2.0572 GiB/s | -17.3    |
| [nfl.csv](examples%2Fnfl.csv)                         | 2.5413 GiB/s | 2.0658 GiB/s | 23.0     |
| customers-2000000.csv (not committable, too large)    | 3.6090 GiB/s | 1.6854 GiB/s | 114.1    |

Ran on an AMD Ryzen 7 9800x3d with 32GB of RAM, with `RUSTFLAGS="-C target-cpu=native -C target-feature=-avx512f"` for AVX2.