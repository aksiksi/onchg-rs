# onchg

[![Crates.io](https://img.shields.io/crates/v/onchg)](https://crates.io/crates/onchg)
[![docs.rs](https://img.shields.io/docsrs/onchg?label=docs.rs)](https://docs.rs/onchg/)
[![codecov](https://codecov.io/gh/aksiksi/onchg-rs/graph/badge.svg?token=CGR9Q13W9Q)](https://codecov.io/gh/aksiksi/onchg-rs)
[![test](https://github.com/aksiksi/onchg-rs/actions/workflows/test.yml/badge.svg)](https://github.com/aksiksi/onchg-rs/actions/workflows/test.yml)

A tool that allows you to keep blocks in sync across different files in your codebase.

## Install

### [pre-commit](https://pre-commit.com/) hook

```yaml
- repo: https://github.com/aksiksi/onchg-rs
  rev: v0.1.5
  hooks:
    - id: onchg
```

### CLI

```
cargo install onchg
```

## Tutorials

### Setup

Create an empty directory:

```
mkdir -p /tmp/onchg/quickstart && cd /tmp/onchg/quickstart
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

Create a Git repo and commit both files:

```
git init . && git add . && git commit -m "first commit"
```

### pre-commit

Create the `pre-commit` config and install the hook:

```
cat >.pre-commit-config.yaml <<EOL
repos:
  - repo: https://github.com/aksiksi/onchg-rs
    rev: v0.1.5
    hooks:
      - id: onchg
EOL

pre-commit install
```

Change `header.h`:

```diff
--- a/header.h
+++ b/header.h
@@ -5,6 +5,7 @@ typedef enum {
     MAIN = 1,
     PRIMARY = 2,
     OTHER = 3,
+    NEW = 4,
 } supported_services_t;
 // LINT.ThenChange(docs.md:supported-services)
```

Stage and commit:

```
$ git add . && git commit -m "my commit"
onchg....................................................................Failed
- hook id: onchg
- exit code: 1

Root path: /home/aksiksi/onchg/quickstart

Parsed 2 files (2 blocks total):
  * /home/aksiksi/onchg/quickstart/docs.md
  * /home/aksiksi/onchg/quickstart/header.h

Violations:
  * block "supported-services" at /home/aksiksi/onchg/quickstart/docs.md:5 (due to block "supported-services" at /home/aksiksi/onchg/quickstart/header.h:2)
```

### CLI

Run `onchg` on the directory:

```
$ onchg directory
Root path: /home/aksiksi/onchg/quickstart

Parsed 2 files (2 blocks total):
  * /home/aksiksi/onchg/quickstart/docs.md
  * /home/aksiksi/onchg/quickstart/header.h

OK.
```

Make a change to the enum in `header.h`:

```diff
--- a/header.h
+++ b/header.h
@@ -5,6 +5,7 @@ typedef enum {
     MAIN = 1,
     PRIMARY = 2,
     OTHER = 3,
+    NEW = 4,
 } supported_services_t;
 // LINT.ThenChange(docs.md:supported-services)
```

Stage the change & run `onchg` in repo mode:

```
$ git add header.h && onchg repo
Root path: /home/aksiksi/onchg/quickstart

Parsed 2 files (2 blocks total):
  * /home/aksiksi/onchg/quickstart/docs.md
  * /home/aksiksi/onchg/quickstart/header.h

Violations:
  * block "supported-services" at /home/aksiksi/onchg/quickstart/docs.md:5 (due to block "supported-services" at /home/aksiksi/onchg/quickstart/header.h:2)
```

Change `docs.md`:

```diff
--- a/docs.md
+++ b/docs.md
@@ -6,5 +6,6 @@
 * Main
 * Primary
 * Other
+* New
 <!-- LINT.ThenChange(header.h:supported-services) -->
```

Stage the change & re-run `onchg`:

```
$ git add docs.md && onchg repo
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
directory-sparse/150    time:   [2.5643 ms 2.5974 ms 2.6338 ms]
grep-sparse/150         time:   [1.6161 ms 1.6241 ms 1.6328 ms]
ripgrep-sparse/150      time:   [4.9077 ms 4.9354 ms 4.9640 ms]

directory-sparse/1000   time:   [15.186 ms 15.271 ms 15.359 ms]
grep-sparse/1000        time:   [6.4750 ms 6.5380 ms 6.6048 ms]
ripgrep-sparse/1000     time:   [7.6132 ms 7.6550 ms 7.6980 ms]
```

### Dense

> [!NOTE]
> This is more of a pathological worst-case benchmark.

50-100 blocks per file. Same line count and line length settings as sparse bench.

5-6x slower than `grep`:

```
directory-dense/150     time:   [11.388 ms 11.469 ms 11.554 ms]
grep-dense/150          time:   [3.1692 ms 3.1907 ms 3.2138 ms]
ripgrep-dense/150       time:   [6.7027 ms 6.7731 ms 6.8621 ms]

directory-dense/1000    time:   [83.987 ms 84.581 ms 85.224 ms]
grep-dense/1000         time:   [15.269 ms 15.349 ms 15.430 ms]
ripgrep-dense/1000      time:   [15.800 ms 15.901 ms 16.004 ms]
```
