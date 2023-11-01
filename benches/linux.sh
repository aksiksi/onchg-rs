#!/bin/bash
[ -d "linux/" ] || git clone --depth 1 https://github.com/torvalds/linux
cd linux/
num_lines=$(find . -type f -exec cat {} + | wc -l)
echo "Number of lines in Linux kernel: ${num_lines}"
time onchg directory .

