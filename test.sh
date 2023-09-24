#!/bin/bash

cargo test -- --nocapture
cargo test --features git -- --nocapture

