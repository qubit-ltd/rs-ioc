// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::Arc;
use std::sync::Mutex;

use qubit_ioc::CleanupError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Dependency;
use qubit_ioc::Managed;
use qubit_ioc::ShutdownMode;
use qubit_ioc::ShutdownPhase;
use qubit_ioc::WaitPolicy;

struct Consumer;
struct DependencyService;

#[test]
fn test_asynchronous_consumer_is_confirmed_before_synchronous_dependency_stops() {
    let events = Arc::new(Mutex::new(Vec::<&'static str>::new()));
    let stop_events = Arc::clone(&events);
    let request_events = Arc::clone(&events);
    let wait_events = Arc::clone(&events);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_factory::<DependencyService, _>(&[], move |_| {
            Ok(Managed::synchronous(Arc::new(DependencyService), move |_| {
                stop_events.lock().expect("lock events").push("stop-dependency");
                Ok(())
            }))
        })
        .expect("register dependency");
    builder
        .register_managed_factory::<Consumer, _>(&[Dependency::of::<DependencyService>()], move |_| {
            Ok(Managed::asynchronous_with_graceful(
                Arc::new(Consumer),
                |_| Ok(()),
                move |_| {
                    request_events.lock().expect("lock events").push("request-consumer");
                    Ok(())
                },
                move |_| {
                    let events = Arc::clone(&wait_events);
                    Box::pin(async move {
                        events.lock().expect("lock events").push("wait-consumer");
                        Ok(())
                    })
                },
            ))
        })
        .expect("register consumer");
    builder.root::<Consumer>();
    let application = builder.build().expect("build application");
    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("create runtime");
    let report = runtime.block_on(shutdown.wait()).expect("complete shutdown");
    assert!(report.is_success());
    assert_eq!(
        *events.lock().expect("lock events"),
        ["request-consumer", "wait-consumer", "stop-dependency"]
    );
}

#[test]
fn test_synchronous_stop_success_completes_shutdown() {
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_factory::<DependencyService, _>(&[], |_| {
            Ok(Managed::synchronous(Arc::new(DependencyService), |_| Ok(())))
        })
        .expect("register dependency");
    builder.root::<DependencyService>();
    let application = builder.build().expect("build application");
    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("create runtime");
    let report = runtime.block_on(shutdown.wait()).expect("complete shutdown");
    assert!(report.is_complete());
    assert!(report.is_success());
}

#[test]
fn test_synchronous_graceful_request_still_runs_terminating_stop() {
    let events = Arc::new(Mutex::new(Vec::<&'static str>::new()));
    let graceful_events = Arc::clone(&events);
    let stop_events = Arc::clone(&events);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_factory::<DependencyService, _>(&[], move |_| {
            Ok(Managed::synchronous_with_graceful(
                Arc::new(DependencyService),
                move |_| {
                    stop_events.lock().expect("lock events").push("stop");
                    Ok(())
                },
                move |_| {
                    graceful_events.lock().expect("lock events").push("graceful");
                    Ok(())
                },
            ))
        })
        .expect("register dependency");
    builder.root::<DependencyService>();
    let application = builder.build().expect("build application");
    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("create runtime");
    let report = runtime.block_on(shutdown.wait()).expect("complete shutdown");
    assert!(report.is_complete());
    assert_eq!(*events.lock().expect("lock events"), ["graceful", "stop"]);
}

#[test]
fn test_synchronous_graceful_request_does_not_hide_stop_failure() {
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_factory::<DependencyService, _>(&[], |_| {
            Ok(Managed::synchronous_with_graceful(
                Arc::new(DependencyService),
                |_| Err(CleanupError::new(std::io::Error::other("stop failed"))),
                |_| Ok(()),
            ))
        })
        .expect("register dependency");
    builder.root::<DependencyService>();
    let application = builder.build().expect("build application");
    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("create runtime");
    let error = runtime.block_on(shutdown.wait()).expect_err("stop must fail");
    assert!(!error.report().is_complete());
    assert_eq!(error.report().failures().len(), 1);
    assert_eq!(error.report().failures()[0].phase, ShutdownPhase::Abort);
}
