csimdv
======

An alternate approach to SIMD CSV parsing, heavily inspired by: https://github.com/medialab/simd-csv

Differences
----------

`simd-csv` is a fantastic library, however, as noted in the README, does *not* use the ["pclmulqdq"](https://branchfree.org/2019/03/06/code-fragment-finding-quote-pairs-with-carry-less-multiply-pclmulqdq/) trick that
many other SIMD based parsers do (most notably simdjson). To be fair, this trick does not work on all targets, which is
the state reason that `simd-csv` does not use it. However, I wanted to see if I could make a version of `simd-csv` that 
did use this trick, and see how much of a performance boost it would give.

I also chose to use `portable_simd`, which requires Rust nightly builds, to make the SIMD code more readable. However,
I end up using intrinsics in the important parts of the implementation, so I will likely drop the dependency there soon.

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

