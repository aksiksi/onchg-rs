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

## Quickstart

### Video

https://www.loom.com/share/4018aea2378f4e4e8fcd403a70749cde?sid=19f4c8ec-87b6-4eac-a448-f326695189ee

### Setup

Create an empty directory:

```
mkdir -p /tmp/onchg/quickstart && cd /tmp/onchg/quickstart
```

Create two files - `docs.md` and `header.h`:

**`docs.md`**:

```markdown
cat >docs.md <<EOL
# Docs

## Supported Services

<!--- LINT.OnChange(supported-services) --->
* Main
* Primary
* Other
<!--- LINT.ThenChange(header.h:supported-services) --->

EOL
```

**`header.h`**:

```c
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

Initialize a Git repo and commit both files:

```
git init . && git add . && git commit -m "first commit"
```

### pre-commit

Create a `pre-commit` config and install the hook:

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

## Documentation

### Examples

#### Two-way Dependency

`alpha.txt`:

```
OnChange(my-block)

ThenChange(beta.txt:their-block)
```

`beta.txt`:

```
OnChange(their-block)

ThenChange(alpha.txt:my-block)
```

#### Relative Paths

`alpha.txt`:

```
OnChange(my-block)

ThenChange(subdir/beta.txt:their-block)
```

`subdir/beta.txt`:

```
OnChange(their-block)

ThenChange(../alpha.txt:my-block)
```

#### Root Paths

`alpha.txt`:

```
OnChange(my-block)

ThenChange(subdir/beta.txt:their-block)
```

`subdir/beta.txt`:

```
OnChange(their-block)

ThenChange(//alpha.txt:my-block)
```

#### One-way OnChange and ThenChange

`alpha.txt`:

```
OnChange()

ThenChange(beta.txt:their-block)
```

`beta.txt`:

```
OnChange(their-block)

ThenChange()
```

#### Multiple Dependencies

`alpha.txt`:

```
OnChange(my-block)

ThenChange(beta.txt:their-block, gamma.txt:another)
```

`beta.txt`:

```
OnChange(their-block)

ThenChange()
```

`gamma.txt`:

```
OnChange(another)

ThenChange(alpha.txt:my-block, beta.txt:their-block)
```

#### Nested Blocks

`alpha.txt`:

```
OnChange(my-block)

OnChange(inner-block)

ThenChange(beta.txt:their-block)

ThenChange()
```

`beta.txt`:

```
OnChange(their-block)

ThenChange()
```

### Details

`onchg` uses **blocks** to capture depdendencies between sections of code (or more generally text) across different files.

A block looks like this:

```
OnChange( [name] )

ThenChange( [<target>[, ...]] )
```

The `OnChange` and `ThenChange` sections can exist anywhere on a line. This allows you to place the sections inside any type of code comment.

`OnChange` accepts an optional `name`. If a block does not specify a name, it cannot be used as a target by other blocks. This is useful in cases where you want one-way dependencies - i.e., if this block changes, other blocks should change, but not vice-versa.

`ThenChange` accepts zero or more `target`s. A block target has the following syntax:

```
[file][:[block]]
```

Just like `OnChange`, `ThenChange` allows for one-way dependencies if the target list is empty.

If a target is specified, it can either be a file or a block in a file. The block is just the block name. The file path must be one of the following:

1. Relative: The path is relative to the current file's path (e.g., `abc/hello.txt`).
2. Relative to the root: The path starts with `//` to indicate that the path is relative to the root directory. This is the path you specify when running `onchg`. Typically, the root would be the Git repo root.

## Benchmarks

### Synthetic

**Setup:**

* **OS**: Ubuntu 22.04 VM
* **CPU**: 10-core AMD 3900x equivalent (virtualized)
* **Disk**: Corsair Force MP510 PCIe Gen3 NVMe drive

`150` and `1000` in the bench names refer to the number of files analyzed.

When compared to `grep`, in addition to finding matches in all files, `onchg` needs to:

1. Load the state of all blocks into memory.
2. Parse and extract the capture group content to ensure that blocks are valid.
3. Run a validation step across all parsed blocks.

> [!NOTE]
> All benchmarks are seeded to allow for reproducibility.

#### Sparse

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

#### Dense

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

#### Git Repo

> [!NOTE]
> This is also a pathological worst-case benchmark. Blocks randomly depend on other
> blocks and the graph is large even with relatively few changed blocks.

This is the same as the dense bench above, but we instead randomly modify *200 blocks*,
stage them, and run `onchg repo`. The bench ends up parsing ~9500 files; the degree of block/file
connectivity is quite high as noted above. Note that the bench generates a total of ~75000 blocks
across 1000 files.

```
git-repo/200            time:   [339.91 ms 340.70 ms 341.53 ms]
```

##### Why so much slower??

Two reasons:

1. The file walk is **single-threaded**.
2. It takes a whopping 250ms just to render the staged diff to stdout!

One interesting finding: when using `libgit2` via the `git` feature, the bench takes ~250ms longer. After
digging into it a bit, it seems that the source of the delay is the diff line iterator in `libgit2`. Somehow,
it takes 2x longer than rendering the diff to stdout and parsing it!

### Real Codebases

#### Linux

> [!WARNING]
> This clones the full Linux kernel tree (~2GB) to the current working directory.

```
$ ./benches/linux.sh
Number of lines in Linux kernel: 38566988
Root path: /home/aksiksi/repos/onchg/linux

Parsed 82259 files (2 blocks total):
  * /home/aksiksi/repos/onchg/linux/.clang-format
  * /home/aksiksi/repos/onchg/linux/.cocciconfig
  * /home/aksiksi/repos/onchg/linux/.get_maintainer.ignore
  * /home/aksiksi/repos/onchg/linux/.gitattributes
  * /home/aksiksi/repos/onchg/linux/.gitignore
  * /home/aksiksi/repos/onchg/linux/.mailmap
  * /home/aksiksi/repos/onchg/linux/.rustfmt.toml
  * /home/aksiksi/repos/onchg/linux/COPYING
  * /home/aksiksi/repos/onchg/linux/CREDITS
  * /home/aksiksi/repos/onchg/linux/Documentation/.gitignore
  * /home/aksiksi/repos/onchg/linux/Documentation/ABI/README
  * /home/aksiksi/repos/onchg/linux/Documentation/ABI/obsolete/o2cb
  * /home/aksiksi/repos/onchg/linux/Documentation/ABI/obsolete/procfs-i8k
  * /home/aksiksi/repos/onchg/linux/Documentation/ABI/obsolete/sysfs-bus-iio
  * /home/aksiksi/repos/onchg/linux/Documentation/ABI/obsolete/sysfs-bus-usb
  ... 82244 files omitted

OK.

real  0m0.628s
user  0m0.779s
sys   0m0.974s
```

