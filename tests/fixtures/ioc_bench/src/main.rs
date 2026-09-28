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
use std::time::Duration;
use std::time::Instant;

use qubit_ioc::ApplicationContext;
use qubit_ioc::BindingId;
use qubit_ioc::BindingKey;
use qubit_ioc::BindingOptions;
use qubit_ioc::ContainerBuilder;

const LOOKUP_ITERATIONS: usize = 10_000;
const ROUNDS: usize = 6;

struct Marker<const INDEX: usize>;

fn main() {
    for unrelated in [0_u32, 16, 128, 1_024, 10_000] {
        for candidates in [1_u32, 8, 64] {
            let primary = (candidates > 1).then_some(0);
            let (context, build_samples) = measured_context(unrelated, candidates, primary);
            print_median(
                &format!("unrelated={unrelated} candidates={candidates} build"),
                &build_samples,
            );
            benchmark_queries(&context, unrelated, candidates, "unique_or_primary");

            if candidates > 1 {
                let (ambiguous, _) = measured_context(unrelated, candidates, None);
                benchmark_ambiguity(&ambiguous, unrelated, candidates);
            }
        }
    }
}

fn measured_context(
    unrelated: u32,
    candidates: u32,
    primary: Option<u32>,
) -> (ApplicationContext, Vec<Duration>) {
    drop(build_context(unrelated, candidates, primary));
    let mut build_samples = Vec::with_capacity(ROUNDS - 1);
    let mut latest = None;
    for _ in 1..ROUNDS {
        let started = Instant::now();
        let context = build_context(unrelated, candidates, primary);
        build_samples.push(started.elapsed());
        latest = Some(context);
    }
    // Return the last context for query measurements and retain all timed
    // construction samples.
    (latest.expect("at least one context was built"), build_samples)
}

fn build_context(unrelated: u32, candidates: u32, primary: Option<u32>) -> ApplicationContext {
    let mut builder = ContainerBuilder::new();
    for index in 0..candidates {
        builder
            .register_instance_with(
                Arc::new(format!("target-{index}")),
                BindingOptions {
                    id: Some(format!("target.t{index}")),
                    primary: primary == Some(index),
                    order: index as i32,
                    ..BindingOptions::default()
                },
            )
            .expect("register target candidate");
    }
    for index in 0..unrelated {
        register_unrelated(&mut builder, index);
    }
    builder.build_all().expect("build benchmark context")
}

fn register_unrelated(builder: &mut ContainerBuilder, index: u32) {
    macro_rules! register_marker {
        ($marker:literal) => {
            builder
                .register_instance_with(
                    Arc::new(Marker::<$marker>),
                    BindingOptions {
                        id: Some(format!("unrelated.u{index}")),
                        ..BindingOptions::default()
                    },
                )
                .expect("register unrelated marker")
        };
    }
    match index % 16 {
        0 => register_marker!(0),
        1 => register_marker!(1),
        2 => register_marker!(2),
        3 => register_marker!(3),
        4 => register_marker!(4),
        5 => register_marker!(5),
        6 => register_marker!(6),
        7 => register_marker!(7),
        8 => register_marker!(8),
        9 => register_marker!(9),
        10 => register_marker!(10),
        11 => register_marker!(11),
        12 => register_marker!(12),
        13 => register_marker!(13),
        14 => register_marker!(14),
        15 => register_marker!(15),
        _ => unreachable!("modulo 16 must stay below 16"),
    }
}

fn unrelated_key(index: u32) -> BindingKey {
    macro_rules! marker_key {
        ($marker:literal) => {
            BindingKey::of::<Marker<$marker>>(Some(
                BindingId::parse(&format!("unrelated.u{index}"))
                    .expect("valid unrelated marker ID"),
            ))
        };
    }
    match index % 16 {
        0 => marker_key!(0),
        1 => marker_key!(1),
        2 => marker_key!(2),
        3 => marker_key!(3),
        4 => marker_key!(4),
        5 => marker_key!(5),
        6 => marker_key!(6),
        7 => marker_key!(7),
        8 => marker_key!(8),
        9 => marker_key!(9),
        10 => marker_key!(10),
        11 => marker_key!(11),
        12 => marker_key!(12),
        13 => marker_key!(13),
        14 => marker_key!(14),
        15 => marker_key!(15),
        _ => unreachable!("modulo 16 must stay below 16"),
    }
}

fn benchmark_queries(context: &ApplicationContext, unrelated: u32, candidates: u32, scenario: &str) {
    let id = format!("target.t{}", candidates - 1);
    let key = BindingKey::of::<String>(Some(BindingId::parse(&id).expect("valid target ID")));
    let missing_id = "target.missing";
    let label = |query: &str| format!("unrelated={unrelated} candidates={candidates} {scenario} {query}");

    measure(&label("get"), LOOKUP_ITERATIONS, || {
        let _ = black_box(context.get::<String>());
    });
    measure(&label("try_get"), LOOKUP_ITERATIONS, || {
        let _ = black_box(context.try_get::<String>());
    });
    measure(&label("get_by_id"), LOOKUP_ITERATIONS, || {
        let _ = black_box(context.get_by_id::<String>(black_box(&id)));
    });
    measure(&label("missing_by_id"), LOOKUP_ITERATIONS, || {
        let _ = black_box(context.get_by_id::<String>(black_box(missing_id)));
    });
    measure(&label("get_all"), LOOKUP_ITERATIONS / 100, || {
        let _ = black_box(context.get_all::<String>());
    });
    measure(&label("binding_sources"), LOOKUP_ITERATIONS, || {
        let _ = black_box(context.binding_sources(black_box(&key)));
    });
    if unrelated > 0 {
        let late_key = unrelated_key(unrelated - 1);
        measure(&label("binding_sources_late_key"), LOOKUP_ITERATIONS, || {
            let _ = black_box(context.binding_sources(black_box(&late_key)));
        });
    }
}

fn benchmark_ambiguity(context: &ApplicationContext, unrelated: u32, candidates: u32) {
    measure(
        &format!("unrelated={unrelated} candidates={candidates} ambiguous get"),
        LOOKUP_ITERATIONS,
        || {
            let _ = black_box(context.get::<String>());
        },
    );
    measure(
        &format!("unrelated={unrelated} candidates={candidates} ambiguous try_get"),
        LOOKUP_ITERATIONS,
        || {
            let _ = black_box(context.try_get::<String>());
        },
    );
}

fn measure(label: &str, iterations: usize, mut action: impl FnMut()) {
    for _ in 0..1_000 {
        action();
    }
    let mut samples = Vec::with_capacity(ROUNDS - 1);
    for _ in 0..(ROUNDS - 1) {
        let started = Instant::now();
        for _ in 0..iterations {
            action();
        }
        samples.push(started.elapsed());
    }
    print_median(&format!("{label} iterations={iterations}"), &samples);
}

fn print_median(label: &str, samples: &[Duration]) {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    println!("{label} samples={samples:?} median={:?}", sorted[sorted.len() / 2]);
}
