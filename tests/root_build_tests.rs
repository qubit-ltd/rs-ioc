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

use qubit_ioc::BuildError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Dependency;
use qubit_ioc::FactoryError;
use qubit_ioc::RegistrationError;

struct Wanted;
struct Unused;
struct Leaf;
struct RootService;
struct MiddleService;
struct MissingService;
struct FailingDependency;
struct SelectedConsumer;
struct UnselectedConsumer;

#[test]
fn test_build_all_runs_every_staged_factory_even_when_a_root_is_registered() {
    let wanted_calls = Arc::new(AtomicUsize::new(0));
    let unused_calls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();

    let calls = Arc::clone(&wanted_calls);
    builder
        .register_factory::<Wanted, _>(&[], move |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(Wanted))
        })
        .expect("stage wanted factory");
    let calls = Arc::clone(&unused_calls);
    builder
        .register_factory::<Unused, _>(&[], move |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(Unused))
        })
        .expect("stage unused factory");
    builder.root::<Wanted>();

    let context = builder.build_all().expect("full graph builds");
    assert_eq!(wanted_calls.load(Ordering::SeqCst), 1);
    assert_eq!(unused_calls.load(Ordering::SeqCst), 1);
    assert!(context.get::<Unused>().is_ok());
}

#[test]
fn test_build_requires_at_least_one_root_before_running_factories() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();
    let factory_calls = Arc::clone(&calls);
    builder
        .register_factory::<Wanted, _>(&[], move |_| {
            factory_calls.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(Wanted))
        })
        .expect("stage wanted factory");

    assert!(matches!(builder.build(), Err(BuildError::NoRootsSelected)));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn test_root_by_id_rejects_invalid_id_at_registration() {
    let mut builder = ContainerBuilder::new();
    assert!(matches!(
        builder.root_by_id::<Wanted>("invalid-id"),
        Err(RegistrationError::InvalidBindingId { error, .. }) if error.value() == "invalid-id"
    ));
}

#[test]
fn test_build_all_accepts_an_empty_graph() {
    let context = ContainerBuilder::new().build_all().expect("empty full graph builds");
    assert!(context.get_all::<Wanted>().is_empty());
}

#[test]
fn test_root_build_constructs_only_the_selected_definition_closure() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(Wanted)).expect("wanted instance");
    let calls_in_factory = Arc::clone(&calls);
    builder
        .register_factory::<Unused, _>(&[], move |_| {
            calls_in_factory.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(Unused))
        })
        .expect("unused factory");
    builder.root::<Wanted>();

    let context = builder.build().expect("root closure builds");
    assert!(context.get::<Wanted>().is_ok());
    assert!(context.get::<Unused>().is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn test_missing_root_reports_available_bindings() {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(Unused)).expect("unused instance");
    builder.root::<Wanted>();
    assert!(matches!(builder.build(), Err(BuildError::MissingRoot { .. })));
}

#[test]
fn test_root_build_missing_dependency_reports_complete_path() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<RootService, _>(&[Dependency::of::<MiddleService>()], |context| {
            let _ = context.get::<MiddleService>().expect("declared dependency");
            Ok(Arc::new(RootService))
        })
        .expect("stage root factory");
    builder
        .register_factory::<MiddleService, _>(&[Dependency::of::<MissingService>()], |_| Ok(Arc::new(MiddleService)))
        .expect("stage middle factory");
    builder.root::<RootService>();

    let error = match builder.build() {
        Ok(_) => panic!("missing nested dependency must fail"),
        Err(error) => error,
    };
    match error {
        BuildError::MissingDependency { path, .. } => {
            assert_eq!(path.len(), 2);
            assert_eq!(path[0].type_name(), std::any::type_name::<RootService>());
            assert_eq!(path[1].type_name(), std::any::type_name::<MiddleService>());
        }
        other => panic!("unexpected build error: {other}"),
    }
}

#[test]
fn test_root_build_factory_failure_ignores_unselected_consumers() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<UnselectedConsumer, _>(&[Dependency::of::<FailingDependency>()], |context| {
            let _ = context.get::<FailingDependency>().expect("declared dependency");
            Ok(Arc::new(UnselectedConsumer))
        })
        .expect("stage unselected consumer");
    builder
        .register_factory::<SelectedConsumer, _>(&[Dependency::of::<FailingDependency>()], |context| {
            let _ = context.get::<FailingDependency>().expect("declared dependency");
            Ok(Arc::new(SelectedConsumer))
        })
        .expect("stage selected consumer");
    builder
        .register_factory::<FailingDependency, _>(&[], |_| {
            Err(FactoryError::new(std::io::Error::other("expected failure")))
        })
        .expect("stage failing dependency");
    builder.root::<SelectedConsumer>();

    let error = match builder.build() {
        Ok(_) => panic!("dependency factory must fail"),
        Err(error) => error,
    };
    match error {
        BuildError::FactoryFailed { path, .. } => {
            assert_eq!(path.len(), 2);
            assert_eq!(path[0].type_name(), std::any::type_name::<SelectedConsumer>());
            assert_eq!(path[1].type_name(), std::any::type_name::<FailingDependency>());
        }
        other => panic!("unexpected build error: {other}"),
    }
}

#[test]
fn test_root_build_duplicate_binding_reports_both_definition_sources() {
    let mut builder = ContainerBuilder::new();
    let first_line = line!() + 1;
    builder.register_instance(Arc::new(Leaf)).expect("stage first leaf");
    let second_line = line!() + 1;
    builder.register_instance(Arc::new(Leaf)).expect("stage second leaf");
    builder
        .register_factory::<RootService, _>(&[Dependency::all::<Leaf>()], |_| Ok(Arc::new(RootService)))
        .expect("stage root service");
    builder.root::<RootService>();

    let error = match builder.build() {
        Ok(_) => panic!("selected duplicate leaves must fail graph validation"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        BuildError::DuplicateBinding { first, second, .. }
            if first.file == file!()
                && second.file == file!()
                && first.line == first_line
                && second.line == second_line
                && first != second
    ));
}
