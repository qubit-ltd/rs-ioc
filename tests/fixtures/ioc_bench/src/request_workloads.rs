// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Independent named requests, distinct from collection dependencies.

use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use qubit_ioc::BindingOptions;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Dependency;

pub(super) const SIZES: [usize; 9] = [1, 2, 4, 8, 16, 32, 64, 256, 1_024];
const SAMPLES: usize = 5;

/// Builds and verifies each independent-request workload without timing it.
pub(super) fn check() {
    for size in SIZES {
        for reverse in [false, true] {
            let calls = Arc::new(AtomicUsize::new(0));
            let application = setup(size, reverse, Some(Arc::clone(&calls)))
                .build()
                .expect("independent requests build");
            let context = application.context();
            let expected = (size as u64) * (size as u64 + 1) / 2;
            assert_eq!(*context.get::<u64>().expect("summary root"), expected);
            assert_eq!(calls.load(Ordering::Relaxed), 1);
            println!("correctness requests size={size} order={} passed", order_name(reverse));
        }
    }
}

/// Reports registration, complete validation/build, and request-loop timings.
pub(super) fn run() {
    for size in SIZES {
        for reverse in [false, true] {
            let ids = request_ids(size, reverse);
            let registration = sample("registration", size, || setup_from_ids(size, &ids, None, None));
            report(size, reverse, "registration", &registration);

            let mut complete_build = Vec::with_capacity(SAMPLES);
            // One untimed warm-up build, followed by five independent samples.
            drop(black_box(
                setup_from_ids(size, &ids, None, None).build().expect("warm-up build"),
            ));
            for _ in 0..SAMPLES {
                let builder = setup_from_ids(size, &ids, None, None);
                let started = Instant::now();
                let application = builder.build().expect("complete build");
                let elapsed = started.elapsed();
                let _application = black_box(application);
                complete_build.push(elapsed);
            }
            report(size, reverse, "validation_build", &complete_build);

            let warmup = setup_from_ids(size, &ids, None, None)
                .build()
                .expect("lookup-loop warm-up build");
            black_box(warmup.context().get::<u64>().expect("resolve warm-up root"));
            drop(warmup);
            let mut lookup = Vec::with_capacity(SAMPLES);
            for _ in 0..SAMPLES {
                let elapsed = Arc::new(AtomicU64::new(0));
                let application = setup_from_ids(size, &ids, None, Some(Arc::clone(&elapsed)))
                    .build()
                    .expect("lookup-loop sample build");
                black_box(application.context().get::<u64>().expect("resolve sample root"));
                let duration = Duration::from_nanos(elapsed.load(Ordering::Relaxed));
                assert!(!duration.is_zero(), "factory request loop must execute");
                lookup.push(duration);
            }
            report(size, reverse, "factory_lookup_loop_arc_clone_and_sum", &lookup);
        }
    }
}

/// Creates names once so request formatting is outside every measured loop.
fn request_ids(size: usize, reverse: bool) -> Vec<String> {
    let indices: Box<dyn Iterator<Item = usize>> = if reverse {
        Box::new((0..size).rev())
    } else {
        Box::new(0..size)
    };
    indices.map(|index| format!("request.r{index}")).collect()
}

/// Stages all individually named `u32` instances.
fn register_instances(size: usize) -> ContainerBuilder {
    let mut builder = ContainerBuilder::new();
    for index in 0..size {
        builder
            .register_instance_with(
                Arc::new((index + 1) as u32),
                BindingOptions {
                    id: Some(format!("request.r{index}")),
                    ..BindingOptions::default()
                },
            )
            .expect("register independent request");
    }
    builder
}

/// Registers named instances and one sum factory rooted as the only root.
fn setup(size: usize, reverse: bool, calls: Option<Arc<AtomicUsize>>) -> ContainerBuilder {
    let ids = request_ids(size, reverse);
    setup_from_ids(size, &ids, calls, None)
}

/// Builds from pre-generated IDs, declaring every request independently.
fn setup_from_ids(
    size: usize,
    ids: &[String],
    calls: Option<Arc<AtomicUsize>>,
    elapsed: Option<Arc<AtomicU64>>,
) -> ContainerBuilder {
    let mut builder = register_instances(size);
    let dependencies: Vec<_> = ids.iter().map(|id| Dependency::with_id::<u32>(id)).collect();
    let factory_ids = ids.to_vec();
    builder
        .register_factory::<u64, _>(&dependencies, move |context| {
            let started = elapsed.as_ref().map(|_| Instant::now());
            let sum = factory_ids
                .iter()
                .map(|id| u64::from(*context.get_by_id::<u32>(id).expect("declared independent request")))
                .sum();
            if let (Some(elapsed), Some(started)) = (elapsed, started) {
                elapsed.store(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
            }
            if let Some(calls) = calls {
                calls.fetch_add(1, Ordering::Relaxed);
            }
            Ok(Arc::new(sum))
        })
        .expect("register aggregate factory");
    builder.root::<u64>();
    builder
}

/// Warms the sample operation once, then records five independent durations.
fn sample<R>(label: &str, size: usize, mut action: impl FnMut() -> R) -> Vec<Duration> {
    drop(black_box(action()));
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let started = Instant::now();
        let _output = black_box(action());
        samples.push(started.elapsed());
    }
    println!("requests size={size} operation={label} warmup=1 samples={SAMPLES}");
    samples
}

/// Prints raw nanosecond samples and their median with the read direction.
fn report(size: usize, reverse: bool, operation: &str, samples: &[Duration]) {
    let raw: Vec<_> = samples.iter().map(Duration::as_nanos).collect();
    let mut sorted = raw.clone();
    sorted.sort_unstable();
    println!(
        "requests size={size} order={} operation={operation} samples_ns={raw:?} median_ns={}",
        order_name(reverse),
        sorted[sorted.len() / 2]
    );
}

/// Returns a stable label for lookup order.
fn order_name(reverse: bool) -> &'static str {
    if reverse { "reverse" } else { "forward" }
}
