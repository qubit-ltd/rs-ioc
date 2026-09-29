// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public-API graph workloads with a separate untimed correctness pass.

use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Instant;

use qubit_ioc::BindingId;
use qubit_ioc::BindingKey;
use qubit_ioc::BindingOptions;
use qubit_ioc::BuildError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Definition;
use qubit_ioc::Dependency;

use crate::SAMPLES;
use crate::SIZES;
use crate::print_samples;

const SMALL_CLOSURE: usize = 8;
const WORKLOADS: [&str; 5] = ["chain", "alias", "collection", "root_closure", "missing_tail"];

/// Shared projection interface for one concrete definition with many aliases.
trait Value: Send + Sync {
    /// Returns the small value held by the concrete allocation.
    fn value(&self) -> u32;
}

impl Value for u32 {
    fn value(&self) -> u32 {
        *self
    }
}

/// Increments an optional untimed correctness counter; timed runs pass `None`.
fn constructed(counter: &Option<Arc<AtomicUsize>>) {
    if let Some(counter) = counter {
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

/// Creates named options for `id`, preserving default profile/primary/order.
fn options(id: String) -> BindingOptions {
    BindingOptions {
        id: Some(id),
        ..BindingOptions::default()
    }
}

/// Registers `n` distinct u32 bindings and selects the first root.
/// The forward chain visits `length` nodes; `missing` requests absent node.nN.
/// Factories only access declared dependencies and return a small Arc value.
fn chain(n: usize, length: usize, missing: bool, counter: Option<Arc<AtomicUsize>>) -> ContainerBuilder {
    let mut builder = ContainerBuilder::new();
    for index in 0..n {
        let next = (index + 1 < length || (missing && index + 1 == length)).then(|| format!("node.n{}", index + 1));
        let dependencies: Vec<_> = next.iter().map(|id| Dependency::with_id::<u32>(id)).collect();
        let calls = counter.clone();
        builder
            .register_factory_with::<u32, _>(&dependencies, options(format!("node.n{index}")), move |context| {
                let value = next
                    .as_deref()
                    .map_or(1, |id| *context.get_by_id::<u32>(id).expect("declared next node") + 1);
                constructed(&calls);
                Ok(Arc::new(value))
            })
            .expect("register named chain node");
    }
    builder.root_by_id::<u32>("node.n0").expect("valid chain root");
    builder
}

/// Registers one deferred concrete value with `n` named aliases in its
/// definition. Rooting one alias selects every member while constructing the
/// concrete once.
fn aliases(n: usize, counter: Option<Arc<AtomicUsize>>) -> ContainerBuilder {
    let mut definition = Definition::<u32>::builder().factory(move |_| {
        constructed(&counter);
        Ok(Arc::new(7))
    });
    for index in 0..n {
        definition = definition.bind::<dyn Value, _>(options(format!("alias.a{index}")), |value| value);
    }
    let mut builder = ContainerBuilder::new();
    builder
        .register_definition(definition.build().expect("valid alias definition"))
        .expect("register alias definition");
    builder.root_by_id::<dyn Value>("alias.a0").expect("valid alias root");
    builder
}

/// Registers `n` u32 candidates and one u64 factory that sums their collection.
/// Each selected factory returns one Arc small value; no I/O is performed.
fn collection(n: usize, counter: Option<Arc<AtomicUsize>>) -> ContainerBuilder {
    let mut builder = ContainerBuilder::new();
    for index in 0..n {
        let calls = counter.clone();
        builder
            .register_factory_with::<u32, _>(&[], options(format!("item.i{index}")), move |_| {
                constructed(&calls);
                Ok(Arc::new((index + 1) as u32))
            })
            .expect("register collection item");
    }
    builder
        .register_factory::<u64, _>(&[Dependency::all::<u32>()], move |context| {
            let sum = context
                .get_all::<u32>()
                .expect("declared collection")
                .iter()
                .map(|value| u64::from(**value))
                .sum();
            constructed(&counter);
            Ok(Arc::new(sum))
        })
        .expect("register collection root");
    builder.root::<u64>();
    builder
}

/// Creates the named workload with size `n`; panics on invalid registration.
fn setup(workload: &str, n: usize, counter: Option<Arc<AtomicUsize>>) -> ContainerBuilder {
    match workload {
        "chain" => chain(n, n, false, counter),
        "alias" => aliases(n, counter),
        "collection" => collection(n, counter),
        "root_closure" => chain(n, SMALL_CLOSURE, false, counter),
        "missing_tail" => chain(n, n, true, counter),
        _ => unreachable!("known graph workload"),
    }
}

/// Checks all sizes outside any timer, including exact counts and error paths.
pub(super) fn check() {
    for n in SIZES {
        for workload in WORKLOADS {
            let counter = Arc::new(AtomicUsize::new(0));
            let result = setup(workload, n, Some(Arc::clone(&counter))).build();
            if workload == "missing_tail" {
                let failure = result.err().expect("missing last dependency must fail");
                match failure.cause() {
                    BuildError::MissingDependency { dependency, path, .. } => {
                        assert_eq!(dependency, &Dependency::with_id::<u32>(&format!("node.n{n}")));
                        assert_eq!(path.len(), n);
                        for (index, key) in path.iter().enumerate() {
                            assert_eq!(
                                key,
                                &BindingKey::of::<u32>(Some(
                                    BindingId::parse(&format!("node.n{index}")).expect("valid path ID"),
                                ))
                            );
                        }
                    }
                    other => panic!("unexpected missing-tail cause: {other:?}"),
                }
                assert_eq!(counter.load(Ordering::Relaxed), 0);
            } else {
                let application = result.expect("correct graph builds");
                let context = application.context();
                let expected_count = match workload {
                    "chain" => {
                        assert_eq!(*context.get_by_id::<u32>("node.n0").expect("first node"), n as u32);
                        assert_eq!(
                            *context
                                .get_by_id::<u32>(&format!("node.n{}", n - 1))
                                .expect("last node"),
                            1
                        );
                        assert_eq!(context.get_all::<u32>().len(), n);
                        n
                    }
                    "alias" => {
                        let concrete = context.get::<u32>().expect("concrete value");
                        let projected: Arc<dyn Value> = concrete;
                        let all = context.get_all::<dyn Value>();
                        assert_eq!(all.len(), n);
                        for alias in all {
                            assert_eq!(alias.value(), 7);
                            assert!(Arc::ptr_eq(&projected, &alias));
                        }
                        1
                    }
                    "collection" => {
                        assert_eq!(
                            *context.get::<u64>().expect("collection sum"),
                            (n as u64) * (n as u64 + 1) / 2
                        );
                        assert_eq!(context.get_all::<u32>().len(), n);
                        n + 1
                    }
                    "root_closure" => {
                        assert_eq!(
                            *context.get_by_id::<u32>("node.n0").expect("small closure root"),
                            SMALL_CLOSURE as u32
                        );
                        assert_eq!(*context.get_by_id::<u32>("node.n7").expect("closure last node"), 1);
                        assert_eq!(context.get_all::<u32>().len(), SMALL_CLOSURE);
                        assert!(context.get_by_id::<u32>(&format!("node.n{}", n - 1)).is_err());
                        SMALL_CLOSURE
                    }
                    _ => unreachable!("successful workload"),
                };
                assert_eq!(counter.load(Ordering::Relaxed), expected_count);
            }
            println!("correctness graph={workload} n={n} passed");
        }
    }
}

/// Times registration separately from graph validation and construction.
/// Warm-up and disposal of Application/BuildFailure occur outside timing.
pub(super) fn run() {
    for n in SIZES {
        for workload in WORKLOADS {
            drop(setup(workload, n, None).build());
            let mut registration = Vec::with_capacity(SAMPLES);
            let mut construction = Vec::with_capacity(SAMPLES);
            for _ in 0..SAMPLES {
                let started = Instant::now();
                let builder = black_box(setup(black_box(workload), black_box(n), None));
                registration.push(started.elapsed());
                let started = Instant::now();
                let result = black_box(builder.build());
                construction.push(started.elapsed());
                drop(result);
            }
            print_samples(&format!("graph={workload} n={n} phase=registration"), &registration);
            print_samples(
                &format!("graph={workload} n={n} phase=validation_construction"),
                &construction,
            );
        }
    }
}
