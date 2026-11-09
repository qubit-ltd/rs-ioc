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
const WORKLOADS: [&str; 5] = [
    "chain",
    "alias",
    "collection",
    "root_closure",
    "missing_tail",
];
const UNREACHABLE_CASES: [(usize, usize); 2] = [(100, 64), (1_000, 1_024)];

/// Shared projection interface for one concrete definition with many aliases.
trait Value: Send + Sync {
    /// Returns the small value held by the concrete allocation.
    fn value(&self) -> u32;
}

/// Candidate type for the unreachable collection workload, distinct from the
/// fixed `u32` root chain and its `usize` consumers.
struct Candidate(u64);

/// Consumer output records the sum of its selected candidate collection.
struct Consumer(usize);

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
fn chain(
    n: usize,
    length: usize,
    missing: bool,
    counter: Option<Arc<AtomicUsize>>,
) -> ContainerBuilder {
    let mut builder = ContainerBuilder::new();
    for index in 0..n {
        let next = (index + 1 < length || (missing && index + 1 == length))
            .then(|| format!("node.n{}", index + 1));
        let dependencies: Vec<_> = next
            .iter()
            .map(|id| Dependency::with_id::<u32>(id))
            .collect();
        let calls = counter.clone();
        builder
            .register_factory_with::<u32, _>(
                &dependencies,
                options(format!("node.n{index}")),
                move |context| {
                    let value = next.as_deref().map_or(1, |id| {
                        *context.get_by_id::<u32>(id).expect("declared next node") + 1
                    });
                    constructed(&calls);
                    Ok(Arc::new(value))
                },
            )
            .expect("register named chain node");
    }
    builder
        .root_by_id::<u32>("node.n0")
        .expect("valid chain root");
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
        definition =
            definition.bind::<dyn Value, _>(options(format!("alias.a{index}")), |value| value);
    }
    let mut builder = ContainerBuilder::new();
    builder
        .register_definition(definition.build().expect("valid alias definition"))
        .expect("register alias definition");
    builder
        .root_by_id::<dyn Value>("alias.a0")
        .expect("valid alias root");
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

/// Registers the fixed root chain plus unreachable all-candidate consumers.
/// One chosen consumer may include an absent exact dependency for the
/// all-active validation control.
fn unreachable_collections(
    consumers: usize,
    candidates: usize,
    selected_consumer: Option<usize>,
    missing_dependency: bool,
    counter: Option<Arc<AtomicUsize>>,
) -> ContainerBuilder {
    let mut builder = chain(8, 8, false, counter.clone());
    for index in 0..candidates {
        let calls = counter.clone();
        builder
            .register_factory_with::<Candidate, _>(
                &[],
                BindingOptions {
                    order: index as i32,
                    ..options(format!("candidate.c{index}"))
                },
                move |_| {
                    constructed(&calls);
                    Ok(Arc::new(Candidate((index + 1) as u64)))
                },
            )
            .expect("register candidate");
    }
    for index in 0..consumers {
        let calls = counter.clone();
        let mut dependencies = vec![Dependency::all::<Candidate>()];
        if missing_dependency && index == 0 {
            dependencies.push(Dependency::with_id::<u8>("absent.consumer_dependency"));
        }
        builder
            .register_factory_with::<Consumer, _>(
                &dependencies,
                options(format!("consumer.c{index}")),
                move |context| {
                    let sum = context
                        .get_all::<Candidate>()
                        .expect("declared candidate collection")
                        .iter()
                        .map(|candidate| candidate.0 as usize)
                        .sum();
                    constructed(&calls);
                    Ok(Arc::new(Consumer(sum)))
                },
            )
            .expect("register collection consumer");
    }
    if let Some(index) = selected_consumer {
        builder
            .root_by_id::<Consumer>(&format!("consumer.c{index}"))
            .expect("valid consumer root");
    }
    builder
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
                    BuildError::MissingDependency {
                        dependency, path, ..
                    } => {
                        assert_eq!(
                            dependency,
                            &Dependency::with_id::<u32>(&format!("node.n{n}"))
                        );
                        assert_eq!(path.len(), n);
                        for (index, key) in path.iter().enumerate() {
                            assert_eq!(
                                key,
                                &BindingKey::of::<u32>(Some(
                                    BindingId::parse(&format!("node.n{index}"))
                                        .expect("valid path ID"),
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
                        assert_eq!(
                            *context.get_by_id::<u32>("node.n0").expect("first node"),
                            n as u32
                        );
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
                            *context
                                .get_by_id::<u32>("node.n0")
                                .expect("small closure root"),
                            SMALL_CLOSURE as u32
                        );
                        assert_eq!(
                            *context
                                .get_by_id::<u32>("node.n7")
                                .expect("closure last node"),
                            1
                        );
                        assert_eq!(context.get_all::<u32>().len(), SMALL_CLOSURE);
                        assert!(
                            context
                                .get_by_id::<u32>(&format!("node.n{}", n - 1))
                                .is_err()
                        );
                        SMALL_CLOSURE
                    }
                    _ => unreachable!("successful workload"),
                };
                assert_eq!(counter.load(Ordering::Relaxed), expected_count);
            }
            println!("correctness graph={workload} n={n} passed");
        }
    }
    for (consumers, candidates) in UNREACHABLE_CASES {
        let counter = Arc::new(AtomicUsize::new(0));
        let application = unreachable_collections(
            consumers,
            candidates,
            None,
            false,
            Some(Arc::clone(&counter)),
        )
        .build()
        .expect("fixed root chain builds without unreachable consumers");
        assert_eq!(counter.load(Ordering::Relaxed), 8);
        let context = application.context();
        assert_eq!(*context.get_by_id::<u32>("node.n0").expect("root chain"), 8);
        assert_eq!(context.get_all::<Candidate>().len(), 0);
        assert!(context.get_by_id::<Consumer>("consumer.c0").is_err());

        let counter = Arc::new(AtomicUsize::new(0));
        let selected = unreachable_collections(
            consumers,
            candidates,
            Some(0),
            false,
            Some(Arc::clone(&counter)),
        )
        .build()
        .expect("selected consumer builds");
        let context = selected.context();
        let all = context.get_all::<Candidate>();
        assert_eq!(all.len(), candidates);
        let sum: usize = all.iter().map(|candidate| candidate.0 as usize).sum();
        assert_eq!(all.first().expect("first candidate").0, 1);
        assert_eq!(all.last().expect("last candidate").0, candidates as u64);
        assert_eq!(
            context
                .get_by_id::<Consumer>("consumer.c0")
                .expect("selected consumer")
                .0,
            sum
        );
        assert_eq!(counter.load(Ordering::Relaxed), 8 + candidates + 1);

        let counter = Arc::new(AtomicUsize::new(0));
        let result = unreachable_collections(
            consumers,
            candidates,
            None,
            true,
            Some(Arc::clone(&counter)),
        )
        .build_all();
        let failure = match result {
            Ok(_) => panic!("all-active validation must reject unreachable missing dependency"),
            Err(failure) => failure,
        };
        assert!(matches!(
            failure.cause(),
            BuildError::MissingDependency { .. }
        ));
        assert_eq!(counter.load(Ordering::Relaxed), 0);
        println!(
            "correctness graph=unreachable_collections consumers={consumers} candidates={candidates} passed"
        );
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
            print_samples(
                &format!("graph={workload} n={n} phase=registration"),
                &registration,
            );
            print_samples(
                &format!("graph={workload} n={n} phase=validation_construction"),
                &construction,
            );
        }
    }
    for (consumers, candidates) in UNREACHABLE_CASES {
        drop(unreachable_collections(consumers, candidates, None, false, None).build());
        let mut registration = Vec::with_capacity(SAMPLES);
        let mut construction = Vec::with_capacity(SAMPLES);
        for _ in 0..SAMPLES {
            let started = Instant::now();
            let builder = black_box(unreachable_collections(
                black_box(consumers),
                black_box(candidates),
                None,
                false,
                None,
            ));
            registration.push(started.elapsed());
            let started = Instant::now();
            let result = black_box(builder.build());
            construction.push(started.elapsed());
            drop(result);
        }
        print_samples(
            &format!(
                "graph=unreachable_collections consumers={consumers} candidates={candidates} phase=registration"
            ),
            &registration,
        );
        print_samples(
            &format!(
                "graph=unreachable_collections consumers={consumers} candidates={candidates} phase=validation_construction"
            ),
            &construction,
        );
    }
}
