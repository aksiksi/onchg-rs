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

0-10 blocks per file with up to 100 (blank) lines per block.

2-3x slower than `grep`:

```
directory-sparse/150    time:   [2.6022 ms 2.6213 ms 2.6424 ms]
                        change: [-3.3353% -1.6370% -0.0899%] (p = 0.05 > 0.05)
                        No change in performance detected.
grep-sparse/150         time:   [1.3956 ms 1.4056 ms 1.4164 ms]
                        change: [-2.0008% -0.1048% +2.0906%] (p = 0.93 > 0.05)
                        No change in performance detected.

directory-sparse/1000   time:   [16.232 ms 16.425 ms 16.646 ms]
                        change: [+1.7347% +3.4955% +5.3931%] (p = 0.00 < 0.05)
                        Performance has regressed.
grep-sparse/1000        time:   [4.9771 ms 5.0276 ms 5.0831 ms]
                        change: [-3.8877% -2.4144% -0.9729%] (p = 0.00 < 0.05)
                        Change within noise threshold.
```

### Dense

> [!NOTE]
> This is more of a pathological worst-case benchmark.

50-100 blocks per file.

10-20x slower than `grep`.

```
directory-dense/150     time:   [12.727 ms 12.849 ms 12.975 ms]
                        change: [-2.2303% -0.9745% +0.2854%] (p = 0.16 > 0.05)
                        No change in performance detected.
grep-dense/150          time:   [1.5796 ms 1.5978 ms 1.6168 ms]
                        change: [+0.0632% +1.5258% +3.3971%] (p = 0.06 > 0.05)
                        No change in performance detected.

directory-dense/1000    time:   [95.927 ms 96.827 ms 97.784 ms]
                        change: [-1.9330% -0.5186% +0.8466%] (p = 0.47 > 0.05)
                        No change in performance detected.
grep-dense/1000         time:   [6.3667 ms 6.4413 ms 6.5192 ms]
                        change: [+2.7580% +4.6161% +6.4787%] (p = 0.00 < 0.05)
                        Performance has regressed.
```
