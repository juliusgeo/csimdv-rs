csimdv
======

An alternate approach to SIMD CSV parsing, heavily inspired by: https://github.com/medialab/simd-csv

Differences
----------

`simd-csv` is a fantastic library, however, as noted in the README, does *not* use the ["pclmulqdq"](https://branchfree.org/2019/03/06/code-fragment-finding-quote-pairs-with-carry-less-multiply-pclmulqdq/) trick that
many other SIMD based parsers do (most notably simdjson). To be fair, this trick does not work on all targets, which is
the state reason that `simd-csv` does not use it. However, I wanted to see if I could make a version of `simd-csv` that 
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
The bulk of the runtime of this parser is spent doing comparisons between the current chunk of data and delimiter, quote, and newline characters.
The target architecture plays a large role in how effective this approach is compared to `simd_csv`. 
I initially implemented this using `portable_simd`, but it results in suboptimal code generation,
especially on aarch64, where there is no equivalent to the `movemask` x86 instruction. I worked around that aspect by using a [trick](https://validark.dev/posts/interleaved-vectors-on-arm/) that results in slightly faster bitmask generation.
Additionally, because the comparisons are done against a fixed chunk of bytes, with varying splats based on the newline, delimiter, etc, 3 vector loads can be avoided by using handwritten intrinsics.

On aarch64, this results in a parser that is roughly 5-15% slower than `simd_csv`, but on x86_64 with AVX-512 support, it can be up to 50% faster.

`aarch64` NEON Performance Comparison
--------------------------------
| File                                                  | `csimdv`      | `simd-csv`    |
|-------------------------------------------------------|---------------|---------------|
| [EDW.TEST_CAL_DT.csv](examples%2FEDW.TEST_CAL_DT.csv) | 2.1462 GiB/s  | 2.0740 GiB/s  |
| [nfl.csv](examples%2Fnfl.csv)                         | 2.0444 GiB/s  | 1.8968 GiB/s  |
| customers-2000000.csv (not committable, too large)    | 1.6593 GiB/s  | 1.7621 GiB/s  |

Ran on an Apple M1 Max with 64GB of RAM.

`x86_64` AVX-512 Performance Comparison
--------------------------------
| File                                                  | `csimdv`     | `simd-csv`    |
|-------------------------------------------------------|--------------|---------------|
| [EDW.TEST_CAL_DT.csv](examples%2FEDW.TEST_CAL_DT.csv) | 2.7538 GiB/s |  1.9902 GiB/s |
| [nfl.csv](examples%2Fnfl.csv)                         | 2.5444 GiB/s | 1.9423 GiB/s  |
| customers-2000000.csv (not committable, too large)    | 2.6522 GiB/s |  1.6383 GiB/s  |

`x86_64` AVX-2 Performance Comparison
--------------------------------
| File                                                  | `csimdv`     | `simd-csv`    |
|-------------------------------------------------------|--------------|---------------|
| [EDW.TEST_CAL_DT.csv](examples%2FEDW.TEST_CAL_DT.csv) | 2.7521 GiB/s |  2.0566 GiB/s |
| [nfl.csv](examples%2Fnfl.csv)                         |  2.5316 GiB/s |  2.0226 GiB/s  |
| customers-2000000.csv (not committable, too large)    | 2.6630 GiB/s |  1.7053 GiB/s  |

Ran on an AMD Ryzen 7 9800x3d with 32GB of RAM, with `RUSTFLAGS="-C target-cpu=native -C target-feature=-avx512f"`.