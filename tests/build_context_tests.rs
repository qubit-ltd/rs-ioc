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
