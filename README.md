# onchg

[![codecov](https://codecov.io/gh/aksiksi/onchg-rs/graph/badge.svg?token=CGR9Q13W9Q)](https://codecov.io/gh/aksiksi/onchg-rs)

A tool that allows you to keep blocks in sync across different files in your codebase.

## Benchmarks

**Setup:** 10 core VM; AMD 3900x equivalent.

**Memory:** ~120MB peak memory usage across all benchmarks.

`150` and `1000` in the bench names refer to the number of files analyzed.

When compared to `grep`, in addition to finding matches in all files, `onchg` needs to:

1. Load the state of all blocks into memory.
2. Parse and extract the capture group content to ensure that blocks are valid.
3. Run a validation step across all parsed blocks.

### Sparse

> [!NOTE]
> This is the more realistic benchmark.

0-10 blocks per file with up to 100 lines per block. Each line varies from 0-100 characters long.

~2x slower than `grep`:

```
directory-sparse/150    time:   [2.9138 ms 2.9558 ms 3.0050 ms]
grep-sparse/150         time:   [1.7046 ms 1.7171 ms 1.7302 ms]
directory-sparse/1000   time:   [17.084 ms 17.294 ms 17.527 ms]
grep-sparse/1000        time:   [7.3685 ms 7.4982 ms 7.6338 ms]
```

### Dense

> [!NOTE]
> This is more of a pathological worst-case benchmark.

50-100 blocks per file. Same line count and line length settings as sparse bench.

5-10x slower than `grep`.

```
directory-dense/150     time:   [15.027 ms 15.239 ms 15.464 ms]
grep-dense/150          time:   [3.4964 ms 3.5651 ms 3.6428 ms]
directory-dense/1000    time:   [113.08 ms 114.64 ms 116.32 ms]
grep-dense/1000         time:   [17.259 ms 17.401 ms 17.548 ms]
```
