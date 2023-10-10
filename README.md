# onchg

[![Crates.io](https://img.shields.io/crates/v/onchg)](https://crates.io/crates/onchg)
[![docs.rs](https://img.shields.io/docsrs/onchg?label=docs.rs)](https://docs.rs/onchg/)
[![codecov](https://codecov.io/gh/aksiksi/onchg-rs/graph/badge.svg?token=CGR9Q13W9Q)](https://codecov.io/gh/aksiksi/onchg-rs)
[![test](https://github.com/aksiksi/onchg-rs/actions/workflows/test.yml/badge.svg)](https://github.com/aksiksi/onchg-rs/actions/workflows/test.yml)

A tool that allows you to keep blocks in sync across different files in your codebase.

## Install

```
cargo install onchg
```

## Tutorial

Create an empty directory:

```
mkdir -p ~/onchg/quickstart && cd ~/onchg/quickstart
```

Create two files - `docs.md` and `header.h`:

**`docs.md`**:

```
cat >docs.md <<EOL
# Docs

## Supported Services

<!-- LINT.OnChange(supported-services) -->
* Main
* Primary
* Other
<!-- LINT.ThenChange(header.h:supported-services) -->

EOL
```

**`header.h`**:

```
cat >header.h <<EOL

// LINT.OnChange(supported-services)
typedef enum {
    INVALID = 0,
    MAIN = 1,
    PRIMARY = 2,
    OTHER = 3,
} supported_services_t;
// LINT.ThenChange(docs.md:supported-services)

EOL
```

Run `onchg` on the directory:

```
$ onchg directory
Root path: /home/aksiksi/onchg/quickstart

Parsed 2 files (2 blocks total):
  * /home/aksiksi/onchg/quickstart/docs.md
  * /home/aksiksi/onchg/quickstart/header.h

OK.
```

Create a Git repo and stage `docs.md`:

```
git init .
git add docs.md
```

Run `onchg` in repo mode:

```
$ onchg repo
Root path: /home/aksiksi/onchg/quickstart

Parsed 2 files (2 blocks total):
  * /home/aksiksi/onchg/quickstart/docs.md
  * /home/aksiksi/onchg/quickstart/header.h

Violations:
  * block "supported-services" in staged file at "/home/aksiksi/onchg/quickstart/docs.md:5" has changed, but its OnChange target block "supported-services" at "/home/aksiksi/onchg/quickstart/header.h:2" has not
```

Stage `header.h` and re-run:

```
$ onchg repo
Root path: /home/aksiksi/onchg/quickstart

Parsed 2 files (2 blocks total):
  * /home/aksiksi/onchg/quickstart/docs.md
  * /home/aksiksi/onchg/quickstart/header.h

OK.
```

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

directory-sparse/150    time:   [2.8788 ms 2.9122 ms 2.9564 ms]
grep-sparse/150         time:   [1.6747 ms 1.6905 ms 1.7096 ms]
ripgrep-sparse/150      time:   [5.1610 ms 5.1990 ms 5.2374 ms]

directory-sparse/1000   time:   [17.001 ms 17.183 ms 17.383 ms]
grep-sparse/1000        time:   [6.7350 ms 6.8130 ms 6.8944 ms]
ripgrep-sparse/1000     time:   [7.9359 ms 8.0190 ms 8.1125 ms]
```

### Dense

> [!NOTE]
> This is more of a pathological worst-case benchmark.

50-100 blocks per file. Same line count and line length settings as sparse bench.

5-10x slower than `grep`:

```
directory-dense/150     time:   [14.406 ms 14.556 ms 14.725 ms]
grep-dense/150          time:   [3.4290 ms 3.4814 ms 3.5367 ms]
ripgrep-dense/150       time:   [7.3262 ms 7.3873 ms 7.4505 ms]

directory-dense/1000    time:   [108.37 ms 109.40 ms 110.57 ms]
grep-dense/1000         time:   [17.434 ms 17.549 ms 17.666 ms]
ripgrep-dense/1000      time:   [18.363 ms 18.517 ms 18.670 ms]
```
