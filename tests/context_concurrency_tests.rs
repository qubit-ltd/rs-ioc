// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Cross-thread queries over the immutable built application context.

use std::cell::Cell;
use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use qubit_ioc::ApplicationContext;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Managed;
use qubit_ioc::ShutdownMode;
use qubit_ioc::WaitPolicy;

/// Polls a future once for callbacks that complete without yielding.
fn run_ready<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("future unexpectedly yielded"),
    }
}

/// Confirms a built context can be shared while all queries read the same Arc.
#[test]
fn test_context_is_send_sync_and_shared_queries_keep_one_instance() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ApplicationContext>();

    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(String::from("shared"))).unwrap();
    let application = builder.build_all().unwrap();
    let context = application.context().clone();
    let expected = context.get::<String>().unwrap();

    std::thread::scope(|scope| {
        for _ in 0..4 {
            let context = context.clone();
            let expected = Arc::clone(&expected);
            scope.spawn(move || {
                let actual = context.get::<String>().unwrap();
                assert!(Arc::ptr_eq(&actual, &expected));
            });
        }
    });

    drop(expected);
    run_ready(application.begin_shutdown(ShutdownMode::Immediate).wait()).unwrap();
}

/// Confirms stop callbacks need to be `Send` but do not need to be `Sync`.
#[test]
fn test_managed_send_callback_can_capture_non_sync_state() {
    let callback_state = Cell::new(0_u32);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_factory::<String, _>(&[], move |_| {
            let callback_state = callback_state.clone();
            Ok(Managed::synchronous(Arc::new(String::from("managed")), move |_| {
                callback_state.set(1);
                Ok(())
            }))
        })
        .unwrap();

    let application = builder.build_all().unwrap();

    let context = application.context().clone();
    let shared = context.clone();
    std::thread::scope(|scope| {
        scope.spawn(move || assert_eq!(shared.get::<String>().unwrap().as_str(), "managed"));
    });
    run_ready(application.begin_shutdown(ShutdownMode::Immediate).wait()).unwrap();
}
