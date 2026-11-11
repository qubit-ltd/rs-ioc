// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use qubit_ioc::BindingOptions;
use qubit_ioc::BuildAccessError;
use qubit_ioc::BuildContext;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Dependency;

struct DropProbe(Arc<AtomicUsize>);

impl Drop for DropProbe {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

struct RetainedContext {
    _context: BuildContext,
    _probe: DropProbe,
}

struct UsedValue {
    _probe: DropProbe,
}
struct UnusedValue {
    _probe: DropProbe,
}

struct RetainedDependencyContext {
    _context: BuildContext,
}

fn assert_undeclared<T>(result: Result<T, BuildAccessError>, expected: Dependency) {
    match result {
        Err(BuildAccessError::UndeclaredDependency { definition, dependency }) => {
            assert_eq!(dependency, expected);
            assert!(definition.file.ends_with("build_context_tests.rs"));
        }
        Ok(_) => panic!("different request identity must be denied"),
    }
}

#[test]
fn test_get_rejects_undeclared_dependency() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_instance(Arc::new(7_u32))
        .expect("stage available value");
    builder
        .register_factory::<u64, _>(&[], |context| {
            let error = context.get::<u32>().expect_err("undeclared read must fail");
            assert!(matches!(error, BuildAccessError::UndeclaredDependency { .. }));
            Ok(Arc::new(9))
        })
        .expect("stage consumer");
    drop(builder.build_all().expect("factory must observe access error"));
}

#[test]
fn test_get_all_reads_only_declared_collection() {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(7_u32)).expect("stage value");
    builder
        .register_factory::<usize, _>(&[Dependency::all::<u32>()], |context| {
            let values = context.get_all::<u32>().expect("declared collection");
            Ok(Arc::new(values.len()))
        })
        .expect("stage collection consumer");
    let application = builder.build_all().expect("valid graph");
    let context = application.context();
    assert_eq!(*context.get::<usize>().expect("built count"), 1);
}

#[test]
fn test_try_get_by_id_distinguishes_missing_and_selected() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<usize, _>(&[Dependency::optional_with_id::<u32>("missing")], |context| {
            assert!(
                context
                    .try_get_by_id::<u32>("missing")
                    .expect("declared optional")
                    .is_none()
            );
            Ok(Arc::new(1))
        })
        .expect("stage optional consumer");
    let application = builder.build_all().expect("optional absence is valid");
    let context = application.context();
    assert_eq!(*context.get::<usize>().expect("built value"), 1);
}

#[test]
fn test_try_get_by_id_returns_the_declared_named_component() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_instance_with(
            Arc::new(37_u32),
            BindingOptions {
                id: Some("present".to_owned()),
                ..BindingOptions::default()
            },
        )
        .expect("stage named value");
    builder
        .register_factory::<u64, _>(&[Dependency::optional_with_id::<u32>("present")], |context| {
            let value = context
                .try_get_by_id::<u32>("present")
                .expect("declared optional lookup")
                .expect("named value exists");
            Ok(Arc::new(u64::from(*value)))
        })
        .expect("stage consumer");
    assert_eq!(
        *builder
            .build_all()
            .expect("valid graph")
            .context()
            .get::<u64>()
            .expect("consumer"),
        37
    );
}

#[test]
fn test_build_context_authorizes_exact_request_identity() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_instance_with(
            Arc::new(7_u32),
            BindingOptions {
                id: Some("present".to_owned()),
                ..BindingOptions::default()
            },
        )
        .expect("stage named value");
    builder
        .register_factory::<u64, _>(
            &[
                Dependency::optional_with_id::<u32>("present"),
                Dependency::optional_with_id::<u32>("missing"),
                Dependency::all::<u16>(),
            ],
            |context| {
                assert_eq!(
                    *context
                        .try_get_by_id::<u32>("present")
                        .expect("declared request")
                        .expect("selected value"),
                    7
                );
                assert!(
                    context
                        .try_get_by_id::<u32>("missing")
                        .expect("declared absence")
                        .is_none()
                );
                assert!(context.get_all::<u16>().expect("declared empty collection").is_empty());

                assert_undeclared(
                    context.get_by_id::<u32>("present"),
                    Dependency::with_id::<u32>("present"),
                );
                assert_undeclared(context.try_get::<u32>(), Dependency::optional::<u32>());
                assert_undeclared(
                    context.try_get_by_id::<u32>("other"),
                    Dependency::optional_with_id::<u32>("other"),
                );
                assert_undeclared(context.get_all::<u32>(), Dependency::all::<u32>());
                assert_undeclared(context.get::<u16>(), Dependency::of::<u16>());
                Ok(Arc::new(1_u64))
            },
        )
        .expect("stage consumer");

    assert_eq!(
        *builder
            .build_all()
            .expect("valid graph")
            .context()
            .get::<u64>()
            .expect("consumer value"),
        1
    );
}

#[test]
fn test_build_context_lookup_across_request_index_threshold() {
    for size in [16, 17] {
        let mut builder = ContainerBuilder::new();
        let ids: Vec<_> = (0..size).map(|index| format!("request_{index}")).collect();
        for (index, id) in ids.iter().enumerate() {
            builder
                .register_instance_with(
                    Arc::new(index as u32),
                    BindingOptions {
                        id: Some(id.clone()),
                        ..BindingOptions::default()
                    },
                )
                .expect("stage named value");
        }
        let requests: Vec<_> = ids.iter().map(|id| Dependency::with_id::<u32>(id)).collect();
        let factory_ids = ids.clone();
        builder
            .register_factory::<u64, _>(&requests, move |context| {
                if size == 17 {
                    assert_undeclared(
                        context.get_by_id::<u32>("not_declared"),
                        Dependency::with_id::<u32>("not_declared"),
                    );
                    assert_undeclared(
                        context.try_get_by_id::<u32>(&factory_ids[0]),
                        Dependency::optional_with_id::<u32>(&factory_ids[0]),
                    );
                    assert_undeclared(context.get_all::<u32>(), Dependency::all::<u32>());
                }
                let total = factory_ids
                    .iter()
                    .map(|id| u64::from(*context.get_by_id::<u32>(id).expect("declared request")))
                    .sum();
                Ok(Arc::new(total))
            })
            .expect("stage aggregate");

        assert_eq!(
            *builder
                .build_all()
                .expect("build requests")
                .context()
                .get::<u64>()
                .expect("aggregate"),
            (size as u64) * (size as u64 - 1) / 2,
        );
    }
}

#[test]
fn test_build_context_collection_order_and_arc_identity_are_preserved() {
    let mut builder = ContainerBuilder::new();
    for (id, order, value) in [("zeta", 0, 3_u32), ("alpha", 0, 1), ("middle", -1, 2)] {
        builder
            .register_instance_with(
                Arc::new(value),
                BindingOptions {
                    id: Some(id.to_owned()),
                    order,
                    ..BindingOptions::default()
                },
            )
            .expect("stage collection member");
    }
    builder
        .register_factory::<Vec<u32>, _>(
            &[Dependency::all::<u32>(), Dependency::with_id::<u32>("alpha")],
            |context| {
                let values = context.get_all::<u32>().expect("declared collection");
                assert_eq!(values.iter().map(|value| **value).collect::<Vec<_>>(), [2, 1, 3]);
                let alpha = context.get_by_id::<u32>("alpha").expect("declared named value");
                assert!(Arc::ptr_eq(&values[1], &alpha));
                Ok(Arc::new(values.into_iter().map(|value| *value).collect()))
            },
        )
        .expect("stage collection consumer");

    assert_eq!(
        builder
            .build_all()
            .expect("valid graph")
            .context()
            .get::<Vec<u32>>()
            .expect("collected values")
            .as_slice(),
        &[2, 1, 3]
    );
}

#[test]
fn test_retained_build_context_does_not_keep_its_container_alive() {
    let drops = Arc::new(AtomicUsize::new(0));
    let factory_drops = Arc::clone(&drops);
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<RetainedContext, _>(&[], move |context| {
            Ok(Arc::new(RetainedContext {
                _context: context,
                _probe: DropProbe(factory_drops),
            }))
        })
        .expect("stage retained context factory");

    let application = builder.build_all().expect("build retained context");
    drop(application);

    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn test_retained_build_context_keeps_only_declared_dependencies_alive() {
    let used_drops = Arc::new(AtomicUsize::new(0));
    let unused_drops = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();
    builder
        .register_instance(Arc::new(UsedValue {
            _probe: DropProbe(Arc::clone(&used_drops)),
        }))
        .expect("stage used dependency");
    builder
        .register_instance(Arc::new(UnusedValue {
            _probe: DropProbe(Arc::clone(&unused_drops)),
        }))
        .expect("stage unused dependency");
    builder
        .register_factory::<RetainedDependencyContext, _>(&[Dependency::of::<UsedValue>()], |context| {
            Ok(Arc::new(RetainedDependencyContext { _context: context }))
        })
        .expect("stage retained dependency context");

    let application = builder.build_all().expect("build all staged components");
    let retained = application
        .context()
        .get::<RetainedDependencyContext>()
        .expect("resolve retained context");
    drop(application);

    assert_eq!(unused_drops.load(Ordering::SeqCst), 1);
    assert_eq!(used_drops.load(Ordering::SeqCst), 0);
    drop(retained);
    assert_eq!(used_drops.load(Ordering::SeqCst), 1);
}
