// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use std::future::Future;
use std::io;
use std::pin::pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use qubit_ioc::BuildError;
use qubit_ioc::CleanupError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::FactoryError;
use qubit_ioc::Managed;
use qubit_ioc::ShutdownMode;
use qubit_ioc::ShutdownPhase;
use qubit_ioc::WaitPolicy;
use qubit_ioc::managed::CleanupFuture;

struct ComponentA;
struct ComponentB;
struct ComponentC;

/// Polls a self-contained shutdown future without selecting a runtime.
fn run_ready<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("shutdown future unexpectedly suspended"),
    }
}

/// Creates a managed value whose wait action records its name.
fn managed_with_wait<T: Send + Sync + 'static>(
    value: T,
    events: Arc<Mutex<Vec<&'static str>>>,
    name: &'static str,
) -> Managed<T> {
    Managed::new(Arc::new(value), |_| Ok(())).with_wait(move |_| {
        Box::pin(async move {
            events.lock().expect("event mutex is available").push(name);
            Ok(())
        })
    })
}

#[test]
fn test_shutdown_collects_wait_callback_panic_and_continues() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<ComponentA, _>(&[], move |_| Ok(managed_with_wait(ComponentA, captured, "wait A")))
        .expect("register component A");
    builder
        .register_managed_factory::<ComponentB, _>(&[], |_| {
            Ok(Managed::new(Arc::new(ComponentB), |_| Ok(()))
                .with_wait(|_| -> CleanupFuture { panic!("create wait future") }))
        })
        .expect("register component B");

    let application = builder.build_all().expect("build components");

    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);
    let error = run_ready(shutdown.wait()).expect_err("wait callback panic is collected");
    assert_eq!(error.report().failures().len(), 1);
    assert_eq!(error.report().failures()[0].phase, ShutdownPhase::Wait);
    assert!(error.report().failures()[0].to_string().contains("create wait future"));
    assert_eq!(*events.lock().expect("event mutex is available"), ["wait A"]);
}

#[test]
fn test_shutdown_collects_wait_poll_panic_and_continues() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<ComponentA, _>(&[], move |_| Ok(managed_with_wait(ComponentA, captured, "wait A")))
        .expect("register component A");
    builder
        .register_managed_factory::<ComponentB, _>(&[], |_| {
            Ok(
                Managed::new(Arc::new(ComponentB), |_| Ok(())).with_wait(|_| -> CleanupFuture {
                    Box::pin(std::future::poll_fn(|_| -> Poll<Result<(), CleanupError>> {
                        panic!("poll wait future")
                    }))
                }),
            )
        })
        .expect("register component B");

    let application = builder.build_all().expect("build components");

    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);
    let error = run_ready(shutdown.wait()).expect_err("wait poll panic is collected");
    assert_eq!(error.report().failures().len(), 1);
    assert_eq!(error.report().failures()[0].phase, ShutdownPhase::Wait);
    assert!(error.report().failures()[0].to_string().contains("poll wait future"));
    assert_eq!(*events.lock().expect("event mutex is available"), ["wait A"]);
}

#[test]
fn test_async_build_failure_collects_wait_panic_and_preserves_cause() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<ComponentA, _>(&[], move |_| Ok(managed_with_wait(ComponentA, captured, "wait A")))
        .expect("register component A");
    builder
        .register_managed_factory::<ComponentB, _>(&[], |_| {
            Ok(Managed::new(Arc::new(ComponentB), |_| Ok(()))
                .with_wait(|_| -> CleanupFuture { panic!("create wait future") }))
        })
        .expect("register component B");
    builder
        .register_async_factory::<ComponentC, _>(&[], |_| {
            Box::pin(async { Err(FactoryError::new(io::Error::other("build failure"))) })
        })
        .expect("register failing component");

    let mut failure = match run_ready(builder.build_all_async()) {
        Ok(_) => panic!("factory error should fail the build"),
        Err(error) => error,
    };
    assert!(matches!(failure.cause(), BuildError::FactoryFailed { .. }));
    let mut cleanup = failure.take_cleanup().expect("constructed components require cleanup");
    let error = run_ready(cleanup.wait()).expect_err("wait panic must be reported");
    let failures = error.report().failures();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].phase, ShutdownPhase::Wait);
    assert!(failures[0].to_string().contains("create wait future"));
    assert_eq!(*events.lock().expect("event mutex is available"), ["wait A"]);
}

#[test]
fn test_cancelled_wait_resumes_then_reports_poll_panic() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let polls = Arc::new(AtomicUsize::new(0));
    let starts = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<ComponentA, _>(&[], move |_| Ok(managed_with_wait(ComponentA, captured, "wait A")))
        .expect("register component A");
    let captured_polls = Arc::clone(&polls);
    let captured_starts = Arc::clone(&starts);
    builder
        .register_managed_factory::<ComponentB, _>(&[], move |_| {
            Ok(
                Managed::new(Arc::new(ComponentB), |_| Ok(())).with_wait(move |_| -> CleanupFuture {
                    captured_starts.fetch_add(1, Ordering::SeqCst);
                    let captured_polls = Arc::clone(&captured_polls);
                    Box::pin(std::future::poll_fn(move |_| {
                        if captured_polls.fetch_add(1, Ordering::SeqCst) == 0 {
                            Poll::Pending
                        } else {
                            panic!("resumed wait future")
                        }
                    }))
                }),
            )
        })
        .expect("register component B");

    let application = builder.build_all().expect("build components");

    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);
    let mut first_wait = Box::pin(shutdown.wait());
    assert!(
        first_wait
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    drop(first_wait);

    let error = run_ready(shutdown.wait()).expect_err("resumed wait panic is collected");
    assert_eq!(error.report().failures().len(), 1);
    assert_eq!(error.report().failures()[0].phase, ShutdownPhase::Wait);
    assert!(error.report().failures()[0].to_string().contains("resumed wait future"));
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert_eq!(*events.lock().expect("event mutex is available"), ["wait A"]);
}
