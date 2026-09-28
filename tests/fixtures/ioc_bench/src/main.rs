// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Reproducible release-mode context query microbenchmark.

use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

use qubit_ioc::BindingOptions;
use qubit_ioc::ContainerBuilder;

fn main() {
    for count in [1_u32, 16, 128, 1_024, 10_000] {
        let mut builder = ContainerBuilder::new();
        for index in 0..count {
            builder
                .register_instance_with(
                    Arc::new(index),
                    BindingOptions {
                        id: Some(format!("item.i{index}")),
                        ..BindingOptions::default()
                    },
                )
                .expect("register benchmark binding");
        }
        let build_started = Instant::now();
        let context = builder.build_all().expect("build benchmark context");
        let build_elapsed = build_started.elapsed();
        let iterations = (1_000_000 / count).max(10_000);
        let id = format!("item.i{}", count - 1);
        let collection_iterations = (iterations / 100).max(100);
        let mut lookup_samples = Vec::new();
        let mut collection_samples = Vec::new();
        for _ in 0..6 {
            let start = Instant::now();
            for _ in 0..iterations {
                black_box(context.get_by_id::<u32>(black_box(&id)).expect("query binding"));
            }
            lookup_samples.push(start.elapsed());
            let start = Instant::now();
            for _ in 0..collection_iterations {
                black_box(context.get_all::<u32>());
            }
            collection_samples.push(start.elapsed());
        }
        println!(
            "bindings={count:4} build={build_elapsed:?} id_lookup_samples={lookup_samples:?} collection_samples={collection_samples:?}"
        );
        lookup_samples.remove(0);
        collection_samples.remove(0);
        lookup_samples.sort();
        collection_samples.sort();
        println!(
            "bindings={count:4} id_lookup={:?}/{} collection={:?}/{} (median of 5)",
            lookup_samples[2], iterations, collection_samples[2], collection_iterations,
        );
    }
}
