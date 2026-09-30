// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Original historical regressions adapted to explicit rollback ownership.

use std::future::Future;
use std::io;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use qubit_event_bus::EventBus;
use qubit_event_bus::EventBusRegistry;
use qubit_execution_services::ExecutionServices;
use qubit_execution_services::ExecutionServicesSubmissionError;
use qubit_ioc::BuildError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Dependency;
use qubit_ioc::FactoryError;
use qubit_ioc::Managed;
use qubit_ioc::WaitPolicy;
use tokio::runtime::Builder;

#[test]
fn test_event_bus_missing_registry_prevents_factory_execution() {
    let factory_calls = Arc::new(AtomicUsize::new(0));
    let captured_calls = Arc::clone(&factory_calls);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_factory::<EventBus, _>(&[Dependency::of::<EventBusRegistry>()], move |_| {
            captured_calls.fetch_add(1, Ordering::SeqCst);
            Err(FactoryError::new(io::Error::other("factory must not run")))
        })
        .expect("register event bus factory");
    builder.root::<EventBus>();

    let failure = builder.build().err().expect("missing registry must fail");
    assert!(matches!(failure.cause(), BuildError::MissingDependency { .. }));
    assert_eq!(factory_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn test_async_build_failure_stops_managed_execution_services_once() {
    let runtime = Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("create test runtime");
    runtime.block_on(async {
        let services = Arc::new(
            ExecutionServices::builder()
                .enable_io()
                .runtime(runtime.handle().clone())
                .build()
                .expect("create execution services"),
        );
        let (task_started_tx, task_started_rx) = tokio::sync::oneshot::channel();
        let _task = services
            .spawn_io(async move {
                task_started_tx.send(()).expect("test receiver should be alive");
                std::future::pending::<()>().await;
                Ok::<(), io::Error>(())
            })
            .expect("submit pending IO task");
        task_started_rx.await.expect("IO task should start");

        let stops = Arc::new(AtomicUsize::new(0));
        let waits = Arc::new(AtomicUsize::new(0));
        let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
        let managed_services = Arc::clone(&services);
        let stop_count = Arc::clone(&stops);
        let wait_count = Arc::clone(&waits);
        builder
            .register_managed_factory::<ExecutionServices, _>(&[], move |_| {
                let stop_count = Arc::clone(&stop_count);
                let wait_count = Arc::clone(&wait_count);
                Ok(Managed::new(Arc::clone(&managed_services), move |services| {
                    stop_count.fetch_add(1, Ordering::SeqCst);
                    let _stop_report = services.stop();
                    Ok(())
                })
                .with_wait(move |services| {
                    wait_count.fetch_add(1, Ordering::SeqCst);
                    Box::pin(async move {
                        services.await_termination().await;
                        Ok(())
                    })
                }))
            })
            .expect("register managed execution services");
        let expected_services = Arc::clone(&services);
        builder
            .register_factory::<u8, _>(&[Dependency::of::<ExecutionServices>()], move |context| {
                let resolved = context.get::<ExecutionServices>().map_err(FactoryError::new)?;
                assert!(Arc::ptr_eq(&resolved, &expected_services));
                Err(FactoryError::new(io::Error::other("expected downstream failure")))
            })
            .expect("register dependent failing factory");
        builder.root::<u8>();

        let mut failure = builder.build_async().await.err().expect("dependent factory must fail");
        assert!(matches!(failure.cause(), BuildError::FactoryFailed { .. }));
        assert_eq!(waits.load(Ordering::SeqCst), 0);
        let mut cleanup = failure.take_cleanup().expect("managed rollback owner");
        let report = cleanup.wait().await.expect("complete historical rollback");
        assert!(report.is_success(), "{report:?}");
        assert_eq!(stops.load(Ordering::SeqCst), 1);
        assert_eq!(waits.load(Ordering::SeqCst), 1);
        assert!(services.is_terminated());
        assert!(matches!(
            services.spawn_io(async { Ok::<(), io::Error>(()) }),
            Err(ExecutionServicesSubmissionError::Rejected { .. })
        ));
    });
}

#[test]
fn test_cancelling_async_build_stops_managed_execution_services_once() {
    let runtime = Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("create test runtime");
    runtime.block_on(async {
        let services = Arc::new(
            ExecutionServices::builder()
                .enable_io()
                .runtime(runtime.handle().clone())
                .build()
                .expect("create execution services"),
        );
        let stops = Arc::new(AtomicUsize::new(0));
        let waits = Arc::new(AtomicUsize::new(0));
        let (factory_started_tx, factory_started_rx) = tokio::sync::oneshot::channel();
        let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
        let managed_services = Arc::clone(&services);
        let stop_count = Arc::clone(&stops);
        let wait_count = Arc::clone(&waits);
        builder
            .register_managed_factory::<ExecutionServices, _>(&[], move |_| {
                let stop_count = Arc::clone(&stop_count);
                let wait_count = Arc::clone(&wait_count);
                Ok(Managed::new(Arc::clone(&managed_services), move |services| {
                    stop_count.fetch_add(1, Ordering::SeqCst);
                    let _stop_report = services.stop();
                    Ok(())
                })
                .with_wait(move |services| {
                    wait_count.fetch_add(1, Ordering::SeqCst);
                    Box::pin(async move {
                        services.await_termination().await;
                        Ok(())
                    })
                }))
            })
            .expect("register managed execution services");
        builder
            .register_async_factory::<u8, _>(&[Dependency::of::<ExecutionServices>()], move |context| {
                let dependency = context.get::<ExecutionServices>();
                Box::pin(async move {
                    let _services = dependency.map_err(FactoryError::new)?;
                    factory_started_tx.send(()).expect("test receiver should be alive");
                    std::future::pending::<()>().await;
                    Ok(Arc::new(1))
                })
            })
            .expect("register pending async factory");
        builder.root::<u8>();

        let mut build = Box::pin(builder.build_async());
        let mut factory_started = Box::pin(factory_started_rx);
        std::future::poll_fn(|context| {
            if build.as_mut().poll(context).is_ready() {
                panic!("pending async factory unexpectedly completed");
            }
            factory_started
                .as_mut()
                .poll(context)
                .map(|result| result.expect("dependent async factory should start"))
        })
        .await;
        drop(build);

        assert_eq!(stops.load(Ordering::SeqCst), 1);
        assert_eq!(waits.load(Ordering::SeqCst), 0);
        services.await_termination().await;
        assert!(services.is_terminated());
    });
}

#[test]
fn test_build_failure_stops_an_already_created_managed_resource() {
    let stops = Arc::new(AtomicUsize::new(0));
    let captured = Arc::clone(&stops);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_factory::<u8, _>(&[], move |_| {
            Ok(Managed::new(Arc::new(1), move |_| {
                captured.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }))
        })
        .expect("register managed resource");
    builder
        .register_factory::<u16, _>(&[], |_| Err(FactoryError::new(io::Error::other("expected failure"))))
        .expect("register failing factory");
    builder.root::<u16>();
    builder.root::<u8>();

    let mut failure = builder.build().err().expect("factory must fail");
    assert!(matches!(failure.cause(), BuildError::FactoryFailed { .. }));
    let mut cleanup = failure.take_cleanup().expect("managed rollback owner");
    let runtime = Builder::new_current_thread().build().expect("create cleanup runtime");
    assert!(runtime.block_on(cleanup.wait()).expect("rollback report").is_success());
    assert_eq!(stops.load(Ordering::SeqCst), 1);
}
