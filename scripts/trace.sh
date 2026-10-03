#!/usr/bin/env bash
set -euo pipefail

RUSTFLAGS='-C target-cpu=native' cargo instruments -t "Branch Misprediction" --release --bench benchmarks --time-limit 600003 -- --bench csimdv
