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
use qubit_ioc::RegistrationError;

struct Wanted;
struct Unused;

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
