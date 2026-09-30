// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public rollback ownership and immediate build failure regressions.

use std::error::Error;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::pin::pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;
use std::time::Duration;

use qubit_ioc::BindingKey;
use qubit_ioc::BuildError;
use qubit_ioc::BuildFailure;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Dependency;
use qubit_ioc::FactoryError;
use qubit_ioc::Managed;
use qubit_ioc::ShutdownMode;
use qubit_ioc::ShutdownPhase;
use qubit_ioc::WaitPolicy;

struct Resource;
struct Failing;

#[test]
fn test_build_async_returns_failure_before_starting_pending_cleanup() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let waits = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    let abort_count = Arc::clone(&aborts);
    let wait_count = Arc::clone(&waits);
    builder
        .register_managed_factory::<Resource, _>(&[], move |_| {
            Ok(Managed::new(Arc::new(Resource), move |_| {
                abort_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .with_wait(move |_| {
                wait_count.fetch_add(1, Ordering::SeqCst);
                Box::pin(std::future::pending())
            }))
        })
        .expect("register resource");
    builder
        .register_factory::<Failing, _>(&[Dependency::of::<Resource>()], |_| {
            Err(FactoryError::new(std::io::Error::other("factory failed")))
        })
        .expect("register failure");
    builder.root::<Failing>();
    let mut build = pin!(builder.build_async());
    let result = build.as_mut().poll(&mut Context::from_waker(Waker::noop()));
    assert!(
        matches!(result, Poll::Ready(Err(_))),
        "build failure must be ready on its first poll without awaiting rollback"
    );
    assert_eq!(
        aborts.load(Ordering::SeqCst),
        1,
        "the completed resource must be aborted exactly once"
    );
    assert_eq!(
        waits.load(Ordering::SeqCst),
        0,
        "the failed build must not start a rollback wait"
    );
}

/// Drives deterministic futures that must finish in one poll.
fn ready<F: Future>(future: F) -> F::Output {
    match pin!(future).as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("expected a ready future"),
    }
}

/// Registers a managed dependency and a fallible consumer for either build
/// mode.
fn failing_builder(aborts: Arc<AtomicUsize>, waits: Arc<AtomicUsize>) -> ContainerBuilder {
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_factory::<Resource, _>(&[], move |_| {
            Ok(Managed::new(Arc::new(Resource), move |_| {
                aborts.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .with_wait(move |_| {
                waits.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(()) })
            }))
        })
        .expect("register resource");
    builder
        .register_factory::<Failing, _>(&[Dependency::of::<Resource>()], |_| {
            Err(FactoryError::new(std::io::Error::other("original failure")))
        })
        .expect("register failing consumer");
    builder.root::<Failing>();
    builder
}

#[test]
fn test_all_build_modes_preserve_cause_and_transfer_cleanup_once() {
    for mode in 0..4 {
        let aborts = Arc::new(AtomicUsize::new(0));
        let waits = Arc::new(AtomicUsize::new(0));
        let builder = failing_builder(Arc::clone(&aborts), Arc::clone(&waits));
        let mut failure = match mode {
            0 => builder.build(),
            1 => builder.build_all(),
            2 => ready(builder.build_async()),
            _ => ready(builder.build_all_async()),
        }
        .err()
        .expect("consumer fails");
        assert_eq!(
            aborts.load(Ordering::SeqCst),
            1,
            "the completed resource must be aborted exactly once in every build mode"
        );
        assert_eq!(
            waits.load(Ordering::SeqCst),
            0,
            "no rollback wait may start before the caller claims the cleanup owner"
        );
        let BuildError::FactoryFailed {
            key,
            path,
            definition,
            error,
        } = failure.cause()
        else {
            panic!("original factory failure remains directly accessible");
        };
        assert_eq!(*key, BindingKey::of::<Failing>(None));
        assert_eq!(*path, [BindingKey::of::<Failing>(None)]);
        assert!(
            definition.to_string().contains("build_failure_tests.rs"),
            "definition must name the failing registration site"
        );
        assert_eq!(
            error
                .source()
                .expect("io source")
                .downcast_ref::<std::io::Error>()
                .expect("original io error")
                .to_string(),
            "original failure"
        );
        assert!(
            failure
                .source()
                .expect("build source")
                .downcast_ref::<BuildError>()
                .is_some(),
            "build error must expose its own source chain"
        );
        assert!(
            format!("{failure:?}").contains("has_cleanup: true"),
            "debug output must advertise the pending rollback owner"
        );
        assert!(
            failure.to_string().contains("cleanup available: true"),
            "display output must advertise the pending rollback owner"
        );
        let mut cleanup = failure.take_cleanup().expect("rollback owner");
        assert!(
            failure.take_cleanup().is_none(),
            "taking the rollback owner twice must not hand out a second handle"
        );
        let (cause, second_cleanup) = failure.into_parts();
        assert!(
            matches!(cause, BuildError::FactoryFailed { .. }),
            "the original factory cause must survive the split"
        );
        assert!(
            second_cleanup.is_none(),
            "the consumed rollback owner must not reappear in into_parts"
        );
        ready(cleanup.wait()).expect("rollback finishes");
        assert_eq!(
            waits.load(Ordering::SeqCst),
            1,
            "the claimed rollback owner must drive exactly one wait"
        );
        drop(cleanup);
        assert_eq!(
            aborts.load(Ordering::SeqCst),
            1,
            "dropping the finished cleanup handle must not abort a second time"
        );
    }
}

#[test]
fn test_dropping_build_failure_does_not_start_wait_or_repeat_abort() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let waits = Arc::new(AtomicUsize::new(0));
    drop(failing_builder(Arc::clone(&aborts), Arc::clone(&waits)).build());
    assert_eq!(
        aborts.load(Ordering::SeqCst),
        1,
        "dropping the failure must still abort the completed resource once"
    );
    assert_eq!(
        waits.load(Ordering::SeqCst),
        0,
        "dropping the failure must not start a rollback wait"
    );
}

#[test]
fn test_graph_failure_has_no_cleanup_and_runs_no_factories() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&calls);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_factory::<Resource, _>(&[Dependency::of::<Failing>()], move |_| {
            count.fetch_add(1, Ordering::SeqCst);
            Ok(Managed::new(Arc::new(Resource), |_| Ok(())))
        })
        .expect("register invalid graph");
    builder.root::<Resource>();
    let mut failure = ready(builder.build_async()).err().expect("missing dependency");
    assert!(
        matches!(failure.cause(), BuildError::MissingDependency { .. }),
        "an unresolvable dependency must fail before any factory runs"
    );
    assert!(
        failure.take_cleanup().is_none(),
        "a graph failure owns nothing, so it must not hand out a rollback handle"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "graph validation must reject the registration before any factory runs"
    );
}

#[test]
fn test_factory_failure_without_managed_resources_has_no_cleanup() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<Failing, _>(&[], |_| Err(FactoryError::new(std::io::Error::other("failed"))))
        .expect("register failure");
    let (_, cleanup) = builder.build_all().err().expect("factory failure").into_parts();
    assert!(
        cleanup.is_none(),
        "a build without managed resources has nothing to roll back"
    );
}

#[test]
fn test_build_failure_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<BuildFailure>();
}

#[test]
fn test_cleanup_timeout_continues_other_waits_and_retains_dependency_store() {
    struct DependencyValue;
    struct Other;
    let dependency = Arc::new(DependencyValue);
    let weak = Arc::downgrade(&dependency);
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::bounded(Duration::ZERO, Duration::ZERO, |_| {
        Box::pin(async {})
    }));
    builder.register_instance(dependency).expect("register dependency");
    let recorded = Arc::clone(&events);
    builder
        .register_managed_factory::<Resource, _>(&[Dependency::of::<DependencyValue>()], move |_| {
            let wait_events = Arc::clone(&recorded);
            Ok(Managed::new(Arc::new(Resource), move |_| {
                recorded.lock().expect("events").push("abort resource");
                Ok(())
            })
            .with_wait(move |_| {
                assert!(
                    weak.upgrade().is_some(),
                    "partial dependency store must survive through cleanup"
                );
                wait_events.lock().expect("events").push("wait resource");
                Box::pin(async { Ok(()) })
            }))
        })
        .expect("resource");
    let recorded = Arc::clone(&events);
    builder
        .register_managed_factory::<Other, _>(&[Dependency::of::<Resource>()], move |_| {
            let wait_events = Arc::clone(&recorded);
            Ok(Managed::new(Arc::new(Other), move |_| {
                recorded.lock().expect("events").push("abort other");
                Ok(())
            })
            .with_wait(move |_| {
                wait_events.lock().expect("events").push("wait other");
                Box::pin(std::future::pending())
            }))
        })
        .expect("other");
    builder
        .register_factory::<Failing, _>(&[Dependency::of::<Other>()], |_| {
            Err(FactoryError::new(std::io::Error::other("failure")))
        })
        .expect("failing consumer");
    builder.root::<Failing>();
    let (_, cleanup) = ready(builder.build_async())
        .err()
        .expect("factory failure")
        .into_parts();
    assert_eq!(*events.lock().expect("events"), ["abort other", "abort resource"]);
    let mut cleanup = cleanup.expect("rollback handle");
    let error = ready(cleanup.wait()).expect_err("pending wait reaches its deadline");
    assert_eq!(error.report().mode(), ShutdownMode::Immediate);
    assert_eq!(error.report().incomplete(), [BindingKey::of::<Other>(None)]);
    assert_eq!(
        error.report().failures().len(),
        1,
        "only the pending component may report a termination-wait failure"
    );
    assert_eq!(error.report().failures()[0].phase, ShutdownPhase::TerminationWait);
    assert_eq!(
        *events.lock().expect("events"),
        ["abort other", "abort resource", "wait other", "wait resource"]
    );
}

#[test]
fn test_factory_panic_propagates_and_only_aborts_completed_resources() {
    for asynchronous in [false, true] {
        let aborts = Arc::new(AtomicUsize::new(0));
        let waits = Arc::new(AtomicUsize::new(0));
        let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
        let abort_count = Arc::clone(&aborts);
        let wait_count = Arc::clone(&waits);
        builder
            .register_managed_factory::<Resource, _>(&[], move |_| {
                Ok(Managed::new(Arc::new(Resource), move |_| {
                    abort_count.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .with_wait(move |_| {
                    wait_count.fetch_add(1, Ordering::SeqCst);
                    Box::pin(async { Ok(()) })
                }))
            })
            .expect("managed dependency");
        builder
            .register_factory::<Failing, _>(&[Dependency::of::<Resource>()], |_| panic!("factory unwind"))
            .expect("panicking factory");
        builder.root::<Failing>();
        let panic = catch_unwind(AssertUnwindSafe(|| {
            let result = if asynchronous {
                ready(builder.build_async())
            } else {
                builder.build()
            };
            result.is_ok()
        }))
        .expect_err("factory panic propagates");
        assert_eq!(panic.downcast_ref::<&str>(), Some(&"factory unwind"));
        assert_eq!(
            aborts.load(Ordering::SeqCst),
            1,
            "the completed resource must be aborted even when a factory unwinds"
        );
        assert_eq!(
            waits.load(Ordering::SeqCst),
            0,
            "an unwinding build must not start a rollback wait"
        );
    }
}

#[test]
fn test_cancelled_build_does_not_create_cleanup_wait() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let waits = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    let abort_count = Arc::clone(&aborts);
    let wait_count = Arc::clone(&waits);
    builder
        .register_managed_factory::<Resource, _>(&[], move |_| {
            Ok(Managed::new(Arc::new(Resource), move |_| {
                abort_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .with_wait(move |_| {
                wait_count.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(()) })
            }))
        })
        .expect("managed dependency");
    builder
        .register_async_factory::<Failing, _>(&[Dependency::of::<Resource>()], |_| Box::pin(std::future::pending()))
        .expect("pending factory");
    builder.root::<Failing>();
    let mut build = Box::pin(builder.build_async());
    assert!(
        build
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending(),
        "a pending async factory must leave the build future pending"
    );
    drop(build);
    assert_eq!(
        aborts.load(Ordering::SeqCst),
        1,
        "cancelling the pending build must still abort the completed resource"
    );
    assert_eq!(
        waits.load(Ordering::SeqCst),
        0,
        "a cancelled build must not create a cleanup wait"
    );
}
