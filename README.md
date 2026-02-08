csimdv
======

An alternate approach to SIMD CSV parsing, heavily inspired by: https://github.com/medialab/simd-csv

Differences
----------

`simd-csv` is a fantastic library, however, as noted in the README, does *not* use the ["pclmulqdq"](https://branchfree.org/2019/03/06/code-fragment-finding-quote-pairs-with-carry-less-multiply-pclmulqdq/) trick that
many other SIMD based parsers do (most notably simdjson). To be fair, this trick does not work on all targets, which is
the state reason that `simd-csv` does not use it. However, I wanted to see if I could make a version of `simd-csv` that 
did use this trick, and see how much of a performance boost it would give.

I also chose to use `portable_simd`, which requires Rust nightly builds, to make the SIMD code more readonable.

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
This is where it gets interesting--I'm sure there are quite a few optimizations I could perform on my existing code. However,
notably, this library is only fast on x86_64 targets. On aarch64, it is roughly 25% slower than `simd-csv`, and on x86_64, it is roughly 50% faster than `simd-csv`.

Now, as to why this performance varies by target, based on benchmarking I believe it is due to better `simd_eq` code generation
on x86_64 targets, which is used heavily to construct the bitmasks that are then used in the `pclmulqdq` step to escape delimiters and newlines inside quotes.
```rust
// find delimiters and quotes
let delimiter_locations = simd_line.simd_eq(dialect.splats.delimiter).to_bitmask();
let quote_locations = simd_line.simd_eq(dialect.splats.quotechar).to_bitmask();
let newline_locations = simd_line.simd_eq(dialect.splats.newline).to_bitmask();
let return_locations = simd_line.simd_eq(dialect.splats.returnchar).to_bitmask();
```
This could be mitigated by using `vpshufb` or `vtbl` to find the locations of the structural characters in fewer instructions,
while also avoiding expensive `simd_eq` calls. However, I have not gotten around to implementing this.