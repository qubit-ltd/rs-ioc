// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Ownership guarantees for managed values, applications, and shutdown handles.

use std::cell::Cell;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use qubit_ioc::Application;
use qubit_ioc::ApplicationContext;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Dependency;
use qubit_ioc::Managed;
use qubit_ioc::ShutdownHandle;
use qubit_ioc::ShutdownMode;
use qubit_ioc::WaitPolicy;

/// Polls immediately ready cleanup without requiring a particular executor.
fn run_ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("cleanup unexpectedly suspended"),
    }
}

/// Builds one managed component while counting callback creation, not polling.
fn counted_builder(aborts: Arc<AtomicUsize>, waits: Arc<AtomicUsize>) -> ContainerBuilder {
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_factory::<(), _>(&[], move |_| {
            Ok(Managed::asynchronous(
                Arc::new(()),
                move |_| {
                    aborts.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
                move |_| {
                    waits.fetch_add(1, Ordering::SeqCst);
                    Box::pin(async { Ok(()) })
                },
            ))
        })
        .expect("register managed component");
    builder
}

#[test]
fn test_untransferred_managed_drop_aborts_once_without_waiting() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let waits = Arc::new(AtomicUsize::new(0));
    let abort_count = Arc::clone(&aborts);
    let wait_count = Arc::clone(&waits);
    let managed = Managed::asynchronous(
        Arc::new("worker"),
        move |value| {
            assert_eq!(*value, "worker");
            abort_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        },
        move |_| {
            wait_count.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(()) })
        },
    );

    drop(managed);

    assert_eq!(aborts.load(Ordering::SeqCst), 1);
    assert_eq!(waits.load(Ordering::SeqCst), 0);
}

#[test]
fn test_untransferred_managed_drop_contains_abort_panic() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let abort_count = Arc::clone(&aborts);
    let managed = Managed::synchronous(Arc::new(()), move |_| {
        abort_count.fetch_add(1, Ordering::SeqCst);
        panic!("abort failed");
    });

    let result = catch_unwind(AssertUnwindSafe(|| drop(managed)));

    assert!(result.is_ok(), "managed drop must contain abort panic");
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
}

#[test]
fn test_untransferred_managed_drop_uses_abort_instead_of_graceful_request() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let graceful_requests = Arc::new(AtomicUsize::new(0));
    let abort_count = Arc::clone(&aborts);
    let graceful_count = Arc::clone(&graceful_requests);
    let managed = Managed::synchronous(Arc::new(()), move |_| {
        abort_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    })
    .with_graceful_stop(move |_| {
        graceful_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    });

    drop(managed);

    assert_eq!(aborts.load(Ordering::SeqCst), 1);
    assert_eq!(graceful_requests.load(Ordering::SeqCst), 0);
}

#[test]
fn test_transfer_preserves_actions_until_application_drop() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let waits = Arc::new(AtomicUsize::new(0));
    let application = counted_builder(Arc::clone(&aborts), Arc::clone(&waits))
        .build_all()
        .expect("construct application");
    let context = application.context().clone();
    assert_eq!(aborts.load(Ordering::SeqCst), 0);
    drop(context.clone());
    assert_eq!(aborts.load(Ordering::SeqCst), 0);

    drop(application);
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
    assert_eq!(waits.load(Ordering::SeqCst), 0);
    drop(context);
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
}

#[test]
fn test_shutdown_handle_drop_aborts_without_creating_wait() {
    for mode in [ShutdownMode::Graceful, ShutdownMode::Immediate] {
        let aborts = Arc::new(AtomicUsize::new(0));
        let waits = Arc::new(AtomicUsize::new(0));
        let application = counted_builder(Arc::clone(&aborts), Arc::clone(&waits))
            .build_all()
            .expect("construct application");
        let handle = application.begin_shutdown(mode);

        drop(handle);

        assert_eq!(aborts.load(Ordering::SeqCst), 1);
        assert_eq!(waits.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn test_successful_graceful_shutdown_discards_unused_abort() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let requests = Arc::new(AtomicUsize::new(0));
    let waits = Arc::new(AtomicUsize::new(0));
    let abort_count = Arc::clone(&aborts);
    let request_count = Arc::clone(&requests);
    let wait_count = Arc::clone(&waits);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_factory::<(), _>(&[], move |_| {
            Ok(Managed::asynchronous(
                Arc::new(()),
                move |_| {
                    abort_count.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
                move |_| {
                    wait_count.fetch_add(1, Ordering::SeqCst);
                    Box::pin(async { Ok(()) })
                },
            )
            .with_graceful_stop(move |_| {
                request_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }))
        })
        .expect("register graceful component");
    let application = builder.build_all().expect("construct graceful application");
    let mut handle = application.begin_shutdown(ShutdownMode::Graceful);

    assert!(
        run_ready(handle.wait())
            .expect("graceful shutdown succeeds")
            .is_success()
    );
    assert!(run_ready(handle.wait()).expect("repeated wait succeeds").is_success());
    handle.abort();
    drop(handle);

    assert_eq!(requests.load(Ordering::SeqCst), 1);
    assert_eq!(waits.load(Ordering::SeqCst), 1);
    assert_eq!(aborts.load(Ordering::SeqCst), 0);
}

#[test]
fn test_repeated_abort_wait_and_drop_never_repeat_callbacks() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let waits = Arc::new(AtomicUsize::new(0));
    let application = counted_builder(Arc::clone(&aborts), Arc::clone(&waits))
        .build_all()
        .expect("construct application");
    let mut handle = application.begin_shutdown(ShutdownMode::Graceful);

    handle.abort();
    handle.abort();
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
    assert_eq!(waits.load(Ordering::SeqCst), 0);
    assert!(run_ready(handle.wait()).expect("wait succeeds").is_success());
    assert!(run_ready(handle.wait()).expect("repeated wait succeeds").is_success());
    handle.abort();
    drop(handle);

    assert_eq!(aborts.load(Ordering::SeqCst), 1);
    assert_eq!(waits.load(Ordering::SeqCst), 1);
}

#[test]
fn test_send_only_callbacks_allow_sending_application_and_handle() {
    let calls = Arc::new(AtomicUsize::new(0));
    let recorded = Arc::clone(&calls);
    let local = Cell::new(0);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_factory::<(), _>(&[], move |_| {
            let wait_local = Cell::new(0);
            Ok(Managed::asynchronous(
                Arc::new(()),
                move |_| {
                    local.set(local.get() + 1);
                    recorded.fetch_add(local.get(), Ordering::SeqCst);
                    Ok(())
                },
                move |_| {
                    wait_local.set(wait_local.get() + 1);
                    assert_eq!(wait_local.get(), 1);
                    Box::pin(async { Ok(()) })
                },
            ))
        })
        .expect("register Send-only callbacks");
    let application = builder.build_all().expect("construct application");
    let handle = std::thread::spawn(move || application.begin_shutdown(ShutdownMode::Immediate))
        .join()
        .expect("application owner can move between threads");
    let report = std::thread::spawn(move || {
        let mut handle = handle;
        run_ready(handle.wait()).expect("shutdown succeeds on another thread")
    })
    .join()
    .expect("shutdown owner can move between threads");

    assert!(report.is_success());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn test_query_context_is_clone_send_sync_and_lifecycle_owners_are_send() {
    /// Verifies the public query handle's cross-thread trait contract.
    fn assert_query_traits<T: Clone + Send + Sync>() {}
    /// Verifies unique lifecycle owners can move between executor threads.
    fn assert_send<T: Send>() {}

    assert_query_traits::<ApplicationContext>();
    assert_send::<Application>();
    assert_send::<ShutdownHandle>();
}

#[test]
fn test_factory_panic_aborts_completed_components_without_creating_wait() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let waits = Arc::new(AtomicUsize::new(0));
    let mut builder = counted_builder(Arc::clone(&aborts), Arc::clone(&waits));
    builder
        .register_factory::<u32, _>(&[Dependency::of::<()>()], |_| panic!("factory panicked"))
        .expect("register panicking dependent factory");

    let outcome = catch_unwind(AssertUnwindSafe(|| builder.build_all().is_ok()));

    assert!(outcome.is_err(), "factory panic propagates after cleanup");
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
    assert_eq!(waits.load(Ordering::SeqCst), 0);
}

#[test]
fn test_cancelled_async_build_aborts_completed_components_without_creating_wait() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let waits = Arc::new(AtomicUsize::new(0));
    let mut builder = counted_builder(Arc::clone(&aborts), Arc::clone(&waits));
    builder
        .register_async_factory::<u32, _>(&[Dependency::of::<()>()], |_| Box::pin(std::future::pending()))
        .expect("register suspended dependent factory");
    let mut build = Box::pin(builder.build_all_async());
    assert!(
        build
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    assert_eq!(aborts.load(Ordering::SeqCst), 0);

    drop(build);

    assert_eq!(aborts.load(Ordering::SeqCst), 1);
    assert_eq!(waits.load(Ordering::SeqCst), 0);
}
