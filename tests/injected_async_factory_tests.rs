// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
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
use qubit_ioc::FactoryError;
use qubit_ioc::Managed;
use qubit_ioc::WaitPolicy;

/// Drives an immediately ready future with a no-op waker.
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("test future unexpectedly suspended"),
    }
}

#[test]
fn injected_async_resolves_required_optional_and_all() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(7_u32))?;
    for (id, value) in [("zeta", "last"), ("alpha", "first")] {
        builder.register_instance_with(
            Arc::<str>::from(value),
            BindingOptions {
                id: Some(id.to_owned()),
                ..BindingOptions::default()
            },
        )?;
    }
    builder
        .register_injected_async_factory::<usize, (Arc<u32>, Option<Arc<u8>>, Vec<Arc<str>>), _>(
            |(number, absent, labels)| {
                Box::pin(async move {
                    assert!(absent.is_none());
                    assert_eq!(
                        labels
                            .iter()
                            .map(|label| label.as_ref())
                            .collect::<Vec<_>>(),
                        ["first", "last"]
                    );
                    Ok(Arc::new(*number as usize + labels.len()))
                })
            },
        )?;

    let application = ready(builder.build_all_async())?;
    assert_eq!(*application.context().get::<usize>()?, 9);
    Ok(())
}

#[test]
fn injected_async_rejects_missing_before_factory_runs() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();
    let captured = Arc::clone(&calls);
    builder
        .register_injected_async_factory::<u64, (Arc<u32>,), _>(move |_| {
            captured.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(Arc::new(1)) })
        })
        .expect("stage injected factory");

    let error = ready(builder.build_all_async())
        .err()
        .expect("missing dependency must fail");
    assert!(matches!(
        error.cause(),
        BuildError::MissingDependency { .. }
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn injected_async_rejects_ambiguous_before_factory_runs() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();
    for (id, value) in [("north", "N"), ("south", "S")] {
        builder
            .register_instance_with(
                Arc::<str>::from(value),
                BindingOptions {
                    id: Some(id.to_owned()),
                    ..BindingOptions::default()
                },
            )
            .expect("stage identified candidate");
    }
    let captured = Arc::clone(&calls);
    builder
        .register_injected_async_factory::<usize, (Arc<str>,), _>(move |_| {
            captured.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(Arc::new(1)) })
        })
        .expect("stage injected factory");

    let error = ready(builder.build_all_async())
        .err()
        .expect("ambiguous dependency must fail");
    assert!(matches!(error.cause(), BuildError::AmbiguousBinding { .. }));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn injected_async_sync_build_preflight() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();
    let captured = Arc::clone(&calls);
    builder
        .register_injected_async_factory::<u32, (), _>(move |_| {
            captured.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(Arc::new(1)) })
        })
        .expect("stage injected async factory");

    let error = builder
        .build_all()
        .err()
        .expect("async factory requires async build");
    assert!(matches!(error.cause(), BuildError::AsyncRequired { .. }));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[derive(Debug, thiserror::Error)]
#[error("later factory failed")]
struct LaterFailure;

#[test]
fn injected_managed_async_rollback() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let abort_count = Arc::clone(&aborts);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_injected_managed_async_factory::<(), (Arc<u32>,), _>(move |_| {
            Box::pin(async move {
                Ok(Managed::asynchronous(
                    Arc::new(()),
                    move |_| {
                        abort_count.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    },
                    |_| Box::pin(async { Ok(()) }),
                ))
            })
        })
        .expect("stage managed injected factory");
    builder
        .register_injected_async_factory::<u64, (Arc<()>,), _>(|_| {
            Box::pin(async { Err(FactoryError::new(LaterFailure)) })
        })
        .expect("stage later failing factory");
    builder
        .register_instance(Arc::new(2_u32))
        .expect("stage dependency");

    let mut failure = ready(builder.build_all_async())
        .err()
        .expect("later factory fails");
    assert!(
        failure
            .cause()
            .source()
            .expect("factory wrapper")
            .source()
            .expect("original factory error")
            .is::<LaterFailure>()
    );
    ready(failure.wait_cleanup());
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
}

#[test]
fn injected_async_cancellation_stops_later_factory() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();
    builder
        .register_injected_async_factory::<u32, (), _>(|_| {
            Box::pin(async {
                std::future::pending::<()>().await;
                Ok(Arc::new(1))
            })
        })
        .expect("stage pending injected factory");
    let later_calls = Arc::clone(&calls);
    builder
        .register_injected_async_factory::<u64, (), _>(move |_| {
            later_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(Arc::new(2)) })
        })
        .expect("stage later factory");

    let mut future = Box::pin(builder.build_all_async());
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(future.as_mut().poll(&mut context).is_pending());
    drop(future);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
