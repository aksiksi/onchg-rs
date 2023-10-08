# onchg

[![codecov](https://codecov.io/gh/aksiksi/onchg-rs/graph/badge.svg?token=CGR9Q13W9Q)](https://codecov.io/gh/aksiksi/onchg-rs)

A tool that allows you to keep blocks in sync across different files in your codebase.

## Benchmarks

~25x slower than `grep`:

```
directory/150           time:   [21.894 ms 22.116 ms 22.343 ms]
                        change: [-2.2853% -0.8431% +0.5307%] (p = 0.24 > 0.05)
                        No change in performance detected.
grep/150                time:   [1.5461 ms 1.5591 ms 1.5740 ms]
                        change: [+4.1214% +5.2821% +6.5803%] (p = 0.00 < 0.05)
                        Performance has regressed.

directory/1000          time:   [138.90 ms 140.19 ms 141.61 ms]
                        change: [-2.1265% -1.0522% -0.0131%] (p = 0.06 > 0.05)
                        No change in performance detected.
grep/1000               time:   [5.7783 ms 5.8586 ms 5.9631 ms]
                        change: [-3.5041% -1.8078% +0.3782%] (p = 0.05 < 0.05)
                        Change within noise threshold.
```
