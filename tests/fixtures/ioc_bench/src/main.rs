// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Reproducible graph and published-context query workloads.
//!
//! Run `correctness` separately before `graph`, `query`, or `all` in release
//! mode. Teardown and assertions are outside measured regions. Each measured
//! workload follows untimed warm-up; no machine-time threshold is imposed.

mod graph_workloads;
mod query_workloads;

use std::time::Duration;
use std::time::Instant;

const SAMPLES: usize = 5;
const SIZES: [usize; 4] = [1_000, 2_000, 4_000, 8_000];

/// Selects the first argument; defaults to correctness and rejects unknown
/// modes.
fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "correctness".to_owned());
    println!("mode={mode} samples={SAMPLES} sizes={SIZES:?}");
    match mode.as_str() {
        "correctness" => {
            graph_workloads::check();
            query_workloads::check();
            println!("correctness=passed");
        }
        "graph" => graph_workloads::run(),
        "query" => query_workloads::run(),
        "all" => {
            graph_workloads::run();
            query_workloads::run();
        }
        _ => panic!("expected correctness, graph, query, or all"),
    }
}

/// Warms up `action`, then times five samples of `iterations` calls each.
/// Callers black-box inputs/outputs and keep setup and assertions outside it.
fn measure(label: &str, iterations: usize, mut action: impl FnMut()) {
    for _ in 0..1_000 {
        action();
    }
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let started = Instant::now();
        for _ in 0..iterations {
            action();
        }
        samples.push(started.elapsed());
    }
    print_samples(&format!("{label} iterations={iterations}"), &samples);
}

/// Prints original nanosecond samples and their median, preserving sample
/// order.
fn print_samples(label: &str, samples: &[Duration]) {
    let raw: Vec<_> = samples.iter().map(Duration::as_nanos).collect();
    let mut sorted = raw.clone();
    sorted.sort_unstable();
    println!("{label} samples_ns={raw:?} median_ns={}", sorted[sorted.len() / 2]);
}
