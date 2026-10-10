// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::error::Error;
use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use qubit_ioc::BindingOptions;
use qubit_ioc::BuildError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Dependency;
use qubit_ioc::FactoryError;
use qubit_spi::ProviderRegistry;
use qubit_spi::ServiceSpec;
use qubit_spi::SyncServiceSpec;
use qubit_spi::error::ProviderResolutionError;

/// Polls a self-contained build future without choosing an executor.
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("fixture future unexpectedly suspended"),
    }
}

#[test]
fn test_graph_errors_precede_any_user_factory() {
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    RUNS.store(0, Ordering::SeqCst);
    let mut missing = ContainerBuilder::new();
    missing
        .register_factory::<u64, _>(&[Dependency::of::<u32>()], |_| {
            RUNS.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(1))
        })
        .expect("stage missing consumer");
    let error = missing.build_all().err().expect("missing dependency");
    assert!(matches!(error.cause(), BuildError::MissingDependency { path, .. } if !path.is_empty()));
    assert_eq!(RUNS.load(Ordering::SeqCst), 0);

    let mut ambiguous = ContainerBuilder::new();
    for id in ["left", "right"] {
        ambiguous
            .register_instance_with(
                Arc::new(1_u32),
                BindingOptions {
                    id: Some(id.to_owned()),
                    ..BindingOptions::default()
                },
            )
            .expect("stage candidate");
    }
    ambiguous
        .register_factory::<u64, _>(&[Dependency::of::<u32>()], |_| {
            RUNS.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(1))
        })
        .expect("stage ambiguous consumer");
    let error = ambiguous.build_all().err().expect("ambiguous dependency");
    assert!(
        matches!(error.cause(), BuildError::AmbiguousBinding { candidates, path, .. }
        if candidates.len() == 2 && !path.is_empty())
    );
    assert_eq!(RUNS.load(Ordering::SeqCst), 0);

    struct CycleA;
    struct CycleB;
    let mut cyclic = ContainerBuilder::new();
    cyclic
        .register_factory::<CycleA, _>(&[Dependency::of::<CycleB>()], |_| {
            RUNS.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(CycleA))
        })
        .expect("stage first node");
    cyclic
        .register_factory::<CycleB, _>(&[Dependency::of::<CycleA>()], |_| {
            RUNS.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(CycleB))
        })
        .expect("stage second node");
    let error = cyclic.build_all().err().expect("dependency cycle");
    assert!(matches!(error.cause(), BuildError::DependencyCycle { path } if path.len() >= 3));
    assert_eq!(RUNS.load(Ordering::SeqCst), 0);
}

#[test]
fn test_mixed_factories_require_async_build_and_share_results() {
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    RUNS.store(0, Ordering::SeqCst);
    /// Builds the same graph for sync rejection and async success.
    fn builder() -> ContainerBuilder {
        let mut builder = ContainerBuilder::new();
        builder
            .register_factory::<u32, _>(&[], |_| {
                RUNS.fetch_add(1, Ordering::SeqCst);
                Ok(Arc::new(3))
            })
            .expect("stage sync factory");
        builder
            .register_async_factory::<u64, _>(&[Dependency::of::<u32>()], |context| {
                let value = context.get::<u32>().expect("declared dependency");
                Box::pin(async move {
                    RUNS.fetch_add(1, Ordering::SeqCst);
                    Ok(Arc::new(u64::from(*value) + 4))
                })
            })
            .expect("stage async factory");
        builder
    }
    assert!(
        matches!(builder().build_all(), Err(failure) if matches!(failure.cause(), BuildError::AsyncRequired { .. }))
    );
    assert_eq!(RUNS.load(Ordering::SeqCst), 0);
    let application = ready(builder().build_all_async()).expect("mixed graph builds asynchronously");
    let context = application.context();
    assert_eq!(*context.get::<u64>().expect("async result"), 7);
    assert_eq!(RUNS.load(Ordering::SeqCst), 2);
}

struct EmptySpec;

impl ServiceSpec for EmptySpec {
    type Config = ();
    type Error = std::io::Error;
}

impl SyncServiceSpec for EmptySpec {
    type Output = String;
}

#[test]
fn test_spi_resolution_error_keeps_its_source_chain() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<String, _>(&[], |_| {
            let registry = ProviderRegistry::<EmptySpec>::default();
            registry
                .resolve()
                .map(|_| Arc::new(String::from("unexpected")))
                .map_err(FactoryError::new)
        })
        .expect("stage SPI factory");
    let error = builder.build_all().err().expect("empty registry cannot resolve");
    assert!(matches!(error.cause(), BuildError::FactoryFailed { path, .. } if path.len() == 1));
    let source = error
        .cause()
        .source()
        .expect("factory error")
        .source()
        .expect("SPI error");
    assert!(
        source
            .downcast_ref::<ProviderResolutionError>()
            .is_some_and(ProviderResolutionError::is_empty_registry)
    );
}
