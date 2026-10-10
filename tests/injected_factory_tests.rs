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
use qubit_ioc::Managed;
use qubit_ioc::WaitPolicy;

#[test]
fn test_injected_factory_supports_zero_and_required_arguments() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_instance(Arc::new(String::from("hello")))
        .expect("register string");
    builder
        .register_injected_factory::<usize, (), _>(|()| Ok(Arc::new(7)))
        .expect("register zero-argument factory");
    builder
        .register_injected_factory::<u32, (Arc<String>,), _>(|(text,)| Ok(Arc::new(text.len() as u32)))
        .expect("register typed factory");
    builder.root::<usize>();
    builder.root::<u32>();

    let application = builder.build().expect("build injected components");

    assert_eq!(*application.context().get::<usize>().expect("usize"), 7);
    assert_eq!(*application.context().get::<u32>().expect("u32"), 5);
}

#[test]
fn test_injected_factory_resolves_optional_and_collection_arguments() {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(11_u32)).expect("register u32");
    builder
        .register_injected_factory::<usize, (Option<Arc<String>>, Vec<Arc<u32>>, Vec<Arc<u64>>), _>(
            |(optional, values, absent)| {
                assert!(optional.is_none());
                assert_eq!(values.len(), 1);
                assert!(absent.is_empty());
                Ok(Arc::new(*values[0] as usize))
            },
        )
        .expect("register optional and collection factory");
    builder.root::<usize>();

    let application = builder.build().expect("build injected component");

    assert_eq!(*application.context().get::<usize>().expect("usize"), 11);
}

#[test]
fn test_repeated_typed_arguments_declare_one_dependency_and_resolve_twice() {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(9_u8)).expect("register value");
    builder
        .register_injected_factory::<u16, (Arc<u8>, Arc<u8>), _>(|(left, right)| {
            assert!(Arc::ptr_eq(&left, &right));
            Ok(Arc::new(u16::from(*left) + u16::from(*right)))
        })
        .expect("register repeated arguments");
    builder.root::<u16>();

    let application = builder.build().expect("repeated request is deduplicated");

    assert_eq!(*application.context().get::<u16>().expect("u16"), 18);
}

#[test]
fn test_injected_factory_supports_eight_arguments() {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(2_u8)).expect("register value");
    builder
        .register_injected_factory::<u16, (Arc<u8>, Arc<u8>, Arc<u8>, Arc<u8>, Arc<u8>, Arc<u8>, Arc<u8>, Arc<u8>), _>(
            |(a, b, c, d, e, f, g, h)| {
                Ok(Arc::new(
                    [a, b, c, d, e, f, g, h].iter().map(|value| u16::from(**value)).sum(),
                ))
            },
        )
        .expect("register eight-argument factory");
    builder.root::<u16>();

    let application = builder.build().expect("build eight-argument factory");

    assert_eq!(*application.context().get::<u16>().expect("u16"), 16);
}

#[test]
fn test_injected_missing_dependency_fails_before_factory_runs() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed_calls = Arc::clone(&calls);
    let mut builder = ContainerBuilder::new();
    builder
        .register_injected_factory::<u64, (Arc<u32>,), _>(move |(value,)| {
            observed_calls.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(u64::from(*value)))
        })
        .expect("registration does not require the dependency yet");
    builder.root::<u64>();

    let failure = match builder.build() {
        Err(failure) => failure,
        Ok(_) => panic!("missing dependency must fail validation"),
    };

    assert!(matches!(failure.cause(), BuildError::MissingDependency { .. }));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn test_injected_managed_factory_participates_in_build_rollback() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let observed_aborts = Arc::clone(&aborts);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_injected_managed_factory::<u32, (), _>(move |()| {
            Ok(Managed::synchronous(Arc::new(3_u32), move |_| {
                observed_aborts.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }))
        })
        .expect("register managed factory");
    builder
        .register_factory::<String, _>(&[Dependency::of::<u32>()], |_| {
            Err(FactoryError::new(std::io::Error::other("later factory failed")))
        })
        .expect("register failing dependent factory");
    builder.root::<String>();

    let failure = match builder.build() {
        Err(failure) => failure,
        Ok(_) => panic!("later factory failure must abort construction"),
    };

    assert!(matches!(failure.cause(), BuildError::FactoryFailed { .. }));
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
}
