// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Observable asynchronous build cancellation through the public API.
use std::error::Error;
use std::future::Future;
use std::future::poll_fn;
use std::io::Error as IoError;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use qubit_ioc::BindingKey;
use qubit_ioc::BuildError;
use qubit_ioc::BuildSessionError;
use qubit_ioc::CleanupError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Dependency;
use qubit_ioc::FactoryError;
use qubit_ioc::Managed;
use qubit_ioc::ShutdownMode;
use qubit_ioc::ShutdownPhase;
use qubit_ioc::WaitPolicy;

struct Resource;
struct Consumer;

/// Polls a deterministic future which must complete immediately.
fn ready<F: Future>(future: F) -> F::Output {
    match Box::pin(future).as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("expected ready future"),
    }
}

/// Creates a completed managed dependency followed by a pending consumer.
fn builder(aborts: Arc<AtomicUsize>, waits: Arc<AtomicUsize>, done: Arc<AtomicBool>) -> ContainerBuilder {
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_factory::<Resource, _>(&[], move |_| {
            Ok(Managed::asynchronous(
                Arc::new(Resource),
                move |_| {
                    aborts.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
                move |_| {
                    waits.fetch_add(1, Ordering::SeqCst);
                    Box::pin(poll_fn(move |_| {
                        if done.load(Ordering::SeqCst) {
                            Poll::Ready(Ok(()))
                        } else {
                            Poll::Pending
                        }
                    }))
                },
            ))
        })
        .expect("resource");
    builder
        .register_async_factory::<Consumer, _>(&[Dependency::of::<Resource>()], |_| Box::pin(std::future::pending()))
        .expect("consumer");
    builder.root::<Consumer>();
    builder
}

#[test]
fn test_unpolled_run_preserves_ready_and_success_transfers_ownership() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&aborts);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_factory::<Resource, _>(&[], move |_| {
            Ok(Managed::synchronous(Arc::new(Resource), move |_| {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }))
        })
        .expect("resource");
    let mut session = builder.build_all_async_session();
    drop(session.run());
    assert_eq!(aborts.load(Ordering::SeqCst), 0);
    let app = ready(session.run()).expect("retry unpolled run");
    assert!(matches!(ready(session.run()), Err(BuildSessionError::AlreadyFinished)));
    assert!(ready(session.wait_cancelled_cleanup()).is_none());
    drop(session);
    assert_eq!(aborts.load(Ordering::SeqCst), 0);
    assert!(
        ready(app.begin_shutdown(ShutdownMode::Immediate).wait())
            .expect("shutdown")
            .is_success()
    );
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
}

#[test]
fn test_cancelled_run_and_wait_resume_once() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let waits = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicBool::new(false));
    let mut session = builder(Arc::clone(&aborts), Arc::clone(&waits), Arc::clone(&done)).build_async_session();
    let mut run = Box::pin(session.run());
    assert!(run.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
    drop(run);
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
    assert_eq!(waits.load(Ordering::SeqCst), 0);
    assert!(matches!(ready(session.run()), Err(BuildSessionError::Cancelled)));
    let mut wait = Box::pin(session.wait_cancelled_cleanup());
    assert!(wait.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
    drop(wait);
    done.store(true, Ordering::SeqCst);
    assert!(ready(session.wait_cancelled_cleanup()).expect("report").is_success());
    assert!(
        ready(session.wait_cancelled_cleanup())
            .expect("same report")
            .is_success()
    );
    assert_eq!(waits.load(Ordering::SeqCst), 1);
    let mut cleanup = session.take_cancelled_cleanup().expect("unique owner");
    assert!(session.take_cancelled_cleanup().is_none());
    assert!(ready(session.wait_cancelled_cleanup()).is_none());
    assert!(ready(cleanup.wait()).expect("stable report").is_success());
    drop(session);
    drop(cleanup);
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
}

#[test]
fn test_run_future_is_send_and_validation_failure_finishes_session() {
    fn assert_send<T: Send>(_: T) {}
    let mut session = ContainerBuilder::new().build_async_session();
    assert_send(session.run());
    assert!(matches!(ready(session.run()), Err(BuildSessionError::Build(_))));
    assert!(matches!(ready(session.run()), Err(BuildSessionError::AlreadyFinished)));
    assert!(session.take_cancelled_cleanup().is_none());
}

#[test]
fn test_cancelled_wait_keeps_original_deadline_and_incomplete_report() {
    let timers = Arc::new(AtomicUsize::new(0));
    let expired = Arc::new(AtomicBool::new(false));
    let count = Arc::clone(&timers);
    let clock = Arc::clone(&expired);
    let policy = WaitPolicy::bounded(
        std::time::Duration::from_secs(1),
        std::time::Duration::from_secs(1),
        move |_| {
            count.fetch_add(1, Ordering::SeqCst);
            let clock = Arc::clone(&clock);
            Box::pin(poll_fn(move |_| {
                if clock.load(Ordering::SeqCst) {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            }))
        },
    );
    let mut session = builder(
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicBool::new(false)),
    )
    .wait_policy(policy)
    .build_async_session();
    let mut run = Box::pin(session.run());
    assert!(run.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
    drop(run);
    let mut wait = Box::pin(session.wait_cancelled_cleanup());
    assert!(wait.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
    drop(wait);
    assert_eq!(timers.load(Ordering::SeqCst), 1);
    expired.store(true, Ordering::SeqCst);
    let report = ready(session.wait_cancelled_cleanup()).expect("timed out report");
    assert_eq!(report.incomplete(), [BindingKey::of::<Resource>(None)]);
    assert!(!report.is_success());
    assert_eq!(timers.load(Ordering::SeqCst), 1);
    assert_eq!(
        ready(session.wait_cancelled_cleanup())
            .expect("stable report")
            .incomplete(),
        report.incomplete()
    );
}

#[test]
fn test_factory_failure_retains_cleanup_in_build_failure() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&aborts);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_factory::<Resource, _>(&[], move |_| {
            Ok(Managed::synchronous(Arc::new(Resource), move |_| {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }))
        })
        .expect("resource");
    builder
        .register_async_factory::<Consumer, _>(&[Dependency::of::<Resource>()], |_| {
            Box::pin(async { Err(FactoryError::new(IoError::other("factory failed"))) })
        })
        .expect("failure");
    builder.root::<Consumer>();
    let mut session = builder.build_async_session();
    let error = ready(session.run()).err().expect("factory failure");
    assert!(
        error
            .source()
            .expect("transparent build cause")
            .downcast_ref::<BuildError>()
            .is_some()
    );
    let BuildSessionError::Build(mut failure) = error else {
        panic!("expected build failure")
    };
    assert!(matches!(failure.cause(), BuildError::FactoryFailed { .. }));
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
    assert!(ready(session.wait_cancelled_cleanup()).is_none());
    assert!(
        ready(failure.wait_cleanup())
            .expect("owned failure cleanup")
            .is_success()
    );
    assert!(matches!(ready(session.run()), Err(BuildSessionError::AlreadyFinished)));
    drop(failure);
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
}

#[test]
fn test_cancelled_cleanup_preserves_abort_and_wait_errors() {
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_factory::<Resource, _>(&[], |_| {
            Ok(Managed::asynchronous(
                Arc::new(Resource),
                |_| Err(CleanupError::new(IoError::other("abort failed"))),
                |_| Box::pin(async { Err(CleanupError::new(IoError::other("wait failed"))) }),
            ))
        })
        .expect("resource");
    builder
        .register_async_factory::<Consumer, _>(&[Dependency::of::<Resource>()], |_| Box::pin(std::future::pending()))
        .expect("pending");
    builder.root::<Consumer>();
    let mut session = builder.build_async_session();
    let mut run = Box::pin(session.run());
    assert!(run.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
    drop(run);
    let report = ready(session.wait_cancelled_cleanup()).expect("error report");
    assert_eq!(report.failures().len(), 2);
    assert_eq!(report.failures()[0].phase, ShutdownPhase::Abort);
    assert_eq!(report.failures()[1].phase, ShutdownPhase::Wait);
    assert!(report.failures()[0].error.to_string().contains("abort failed"));
    assert!(report.failures()[1].error.to_string().contains("wait failed"));
    assert_eq!(
        ready(session.wait_cancelled_cleanup())
            .expect("stable error report")
            .failures()
            .len(),
        2
    );
}

#[test]
fn test_active_factory_drops_before_abort_including_unwind() {
    struct FactoryGuard(Arc<AtomicBool>);
    impl Drop for FactoryGuard {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    for panic in [false, true] {
        let dropped = Arc::new(AtomicBool::new(false));
        let observed = Arc::clone(&dropped);
        let aborts = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&aborts);
        let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
        builder
            .register_managed_factory::<Resource, _>(&[], move |_| {
                Ok(Managed::synchronous(Arc::new(Resource), move |_| {
                    assert!(observed.load(Ordering::SeqCst), "active factory must drop before abort");
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }))
            })
            .expect("resource");
        builder
            .register_async_factory::<Consumer, _>(&[Dependency::of::<Resource>()], move |_| {
                Box::pin(async move {
                    let _guard = FactoryGuard(dropped);
                    assert!(!panic, "factory panic");
                    std::future::pending().await
                })
            })
            .expect("consumer");
        builder.root::<Consumer>();
        let mut session = builder.build_async_session();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut run = Box::pin(session.run());
            assert!(run.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
        }));
        assert_eq!(outcome.is_err(), panic);
        assert_eq!(aborts.load(Ordering::SeqCst), 1);
        assert!(matches!(ready(session.run()), Err(BuildSessionError::Cancelled)));
        assert!(
            ready(session.wait_cancelled_cleanup())
                .expect("cleanup after drop or unwind")
                .is_success()
        );
        drop(session);
        assert_eq!(aborts.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn test_cancelled_before_managed_product_has_no_cleanup() {
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_async_factory::<Resource, _>(&[], |_| Box::pin(std::future::pending()))
        .expect("pending managed factory");
    let mut session = builder.build_all_async_session();
    let mut run = Box::pin(session.run());
    assert!(run.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
    drop(run);
    assert!(matches!(ready(session.run()), Err(BuildSessionError::Cancelled)));
    assert!(ready(session.wait_cancelled_cleanup()).is_none());
    assert!(session.take_cancelled_cleanup().is_none());
}

#[test]
fn test_dropping_session_requests_no_repeated_abort_or_wait() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let waits = Arc::new(AtomicUsize::new(0));
    let mut session = builder(
        Arc::clone(&aborts),
        Arc::clone(&waits),
        Arc::new(AtomicBool::new(false)),
    )
    .build_async_session();
    let mut run = Box::pin(session.run());
    assert!(run.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
    drop(run);
    drop(session);
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
    assert_eq!(waits.load(Ordering::SeqCst), 0);
}
