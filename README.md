# onchg

[![codecov](https://codecov.io/gh/aksiksi/onchg-rs/graph/badge.svg?token=CGR9Q13W9Q)](https://codecov.io/gh/aksiksi/onchg-rs)

A tool that allows you to keep blocks in sync across different files in your codebase.

## Benchmarks

<10x slower than `grep`:

```
directory/150           time:   [11.679 ms 12.027 ms 12.407 ms]
                        change: [+25.735% +31.181% +36.746%] (p = 0.00 < 0.05)
                        Performance has regressed.
grep/150                time:   [1.9698 ms 2.0836 ms 2.2288 ms]
                        change: [-19.843% -15.008% -10.397%] (p = 0.00 < 0.05)
                        Performance has improved.

directory/1000          time:   [64.970 ms 66.166 ms 67.441 ms]
                        change: [-17.439% -14.763% -12.092%] (p = 0.00 < 0.05)
                        Performance has improved.
grep/1000               time:   [6.3918 ms 6.5284 ms 6.6947 ms]
                        change: [-12.508% -9.7924% -7.0339%] (p = 0.00 < 0.05)
                        Performance has improved.
```
