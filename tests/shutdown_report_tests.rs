// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod support;

use std::error::Error;
use std::future::poll_fn;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use qubit_ioc::Application;
use qubit_ioc::ApplicationState;
use qubit_ioc::BindingKey;
use qubit_ioc::CleanupError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Dependency;
use qubit_ioc::Managed;
use qubit_ioc::ShutdownMode;
use qubit_ioc::ShutdownPhase;
use qubit_ioc::WaitPolicy;
use support::shutdown_timer::Gate;
use support::shutdown_timer::Timers;
use support::shutdown_timer::poll_once;
use support::shutdown_timer::ready;

/// Builds a service and a dependency whose stop actions prove traversal
/// continues.
fn with_dependency(service: Managed<u64>, policy: WaitPolicy) -> (Application, Arc<AtomicUsize>) {
    let tail = Arc::new(AtomicUsize::new(0));
    let aborted = Arc::clone(&tail);
    let stopped = Arc::clone(&tail);
    let mut builder = ContainerBuilder::new().wait_policy(policy);
    builder
        .register_managed_factory::<u32, _>(&[], move |_| {
            Ok(Managed::synchronous(Arc::new(1), move |_| {
                aborted.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .with_graceful_stop(move |_| {
                stopped.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }))
        })
        .expect("register dependency");
    builder
        .register_managed_factory::<u64, _>(&[Dependency::of::<u32>()], move |_| Ok(service))
        .expect("register service");
    (builder.build_all().expect("build service with dependency"), tail)
}

#[test]
fn test_original_cleanup_source_survives_report_clones_and_repeated_wait() {
    let managed = Managed::asynchronous(
        Arc::new(2_u64),
        |_| Err(CleanupError::new(std::io::Error::other("original abort error"))),
        |_| Box::pin(async { Ok(()) }),
    );
    let (application, tail) = with_dependency(managed, WaitPolicy::unbounded());
    let context = application.context().clone();
    let mut handle = application.begin_shutdown(ShutdownMode::Immediate);
    let first = ready(handle.wait()).expect_err("abort error report");
    let second = ready(handle.wait()).expect_err("same error report");
    assert_eq!(tail.load(Ordering::SeqCst), 1);
    assert_eq!(context.state(), ApplicationState::Closed);
    assert!(first.report().is_complete());
    assert!(!first.report().is_success());
    assert_eq!(first.report().mode(), ShutdownMode::Immediate);
    assert_eq!(first.report().failures().len(), 1);
    let failure = &first.report().failures()[0];
    assert_eq!(failure.phase, ShutdownPhase::Abort);
    assert_eq!(failure.key, BindingKey::of::<u64>(None));
    let source = failure
        .error
        .source()
        .expect("original source")
        .downcast_ref::<std::io::Error>()
        .expect("io error remains downcastable");
    assert_eq!(source.to_string(), "original abort error");
    assert!(std::ptr::eq(failure, &second.report().failures()[0]));
}

#[test]
fn test_pending_excludes_completed_entries_and_abort_skips_them() {
    let gate = Gate::default();
    let waiting = gate.clone();
    let completed_aborts = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&completed_aborts);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_factory::<u32, _>(&[], move |_| {
            Ok(Managed::asynchronous(
                Arc::new(1),
                |_| Ok(()),
                move |_| {
                    Box::pin(async move {
                        waiting.await;
                        Ok(())
                    })
                },
            )
            .with_graceful_stop(|_| Ok(())))
        })
        .expect("register pending dependency");
    builder
        .register_managed_factory::<u64, _>(&[Dependency::of::<u32>()], move |_| {
            Ok(Managed::asynchronous(
                Arc::new(2),
                move |_| {
                    observed.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
                |_| Box::pin(async { Ok(()) }),
            )
            .with_graceful_stop(|_| Ok(())))
        })
        .expect("register ready consumer");
    let mut handle = builder
        .build_all()
        .expect("build")
        .begin_shutdown(ShutdownMode::Graceful);
    assert_eq!(
        handle.pending(),
        [BindingKey::of::<u64>(None), BindingKey::of::<u32>(None)]
    );
    assert!(poll_once(handle.wait()).is_pending());
    assert_eq!(handle.pending(), [BindingKey::of::<u32>(None)]);
    handle.abort();
    gate.trigger();
    assert!(ready(handle.wait()).expect("all done").is_success());
    assert!(handle.pending().is_empty());
    handle.abort();
    assert!(ready(handle.wait()).expect("cached completion").is_success());
    drop(handle);
    assert_eq!(completed_aborts.load(Ordering::SeqCst), 0);
}

#[test]
fn test_fallbacks_only_include_entries_without_graceful_callback() {
    let managed = Managed::asynchronous(Arc::new(2_u64), |_| Ok(()), |_| Box::pin(async { Ok(()) }));
    let (application, _) = with_dependency(managed, WaitPolicy::unbounded());
    let mut handle = application.begin_shutdown(ShutdownMode::Graceful);
    let report = ready(handle.wait()).expect("fallback success");
    assert_eq!(report.fallbacks(), [BindingKey::of::<u64>(None)]);
    assert_eq!(report.mode(), ShutdownMode::Graceful);
    assert!(report.is_success());
    assert_eq!(
        ready(handle.wait()).expect("cached report").fallbacks(),
        report.fallbacks()
    );
}

#[test]
fn test_termination_timeout_records_incomplete_and_continues_dependencies() {
    let gate = Gate::default();
    let timers = Timers::default();
    let managed = Managed::asynchronous(
        Arc::new(2_u64),
        |_| Ok(()),
        move |_| {
            Box::pin(async move {
                gate.await;
                Ok(())
            })
        },
    )
    .with_graceful_stop(|_| Ok(()));
    let (application, tail) = with_dependency(managed, timers.policy());
    let context = application.context().clone();
    let mut handle = application.begin_shutdown(ShutdownMode::Graceful);
    assert!(poll_once(handle.wait()).is_pending());
    assert_eq!(tail.load(Ordering::SeqCst), 0);
    timers.trigger(0);
    assert!(poll_once(handle.wait()).is_pending());
    assert_eq!(tail.load(Ordering::SeqCst), 0);
    timers.trigger(1);
    let error = ready(handle.wait()).expect_err("deadlines fail");
    assert_eq!(error.report().incomplete(), [BindingKey::of::<u64>(None)]);
    assert_eq!(error.report().failures()[1].phase, ShutdownPhase::TerminationWait);
    assert_eq!(timers.durations(), [Duration::from_secs(13), Duration::from_secs(7)]);
    assert_eq!(tail.load(Ordering::SeqCst), 2);
    assert_eq!(context.state(), ApplicationState::Incomplete);
    assert!(!error.report().is_complete());
}

#[test]
fn test_abandon_does_not_start_any_wait_and_records_pending_entries() {
    let starts = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&starts);
    let managed = Managed::asynchronous(
        Arc::new(2_u64),
        |_| Ok(()),
        move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(()) })
        },
    )
    .with_graceful_stop(|_| Ok(()));
    let (application, tail) = with_dependency(managed, WaitPolicy::unbounded());
    let context = application.context().clone();
    let report = application.begin_shutdown(ShutdownMode::Graceful).abandon();
    assert_eq!(report.incomplete(), [BindingKey::of::<u64>(None)]);
    assert_eq!(starts.load(Ordering::SeqCst), 0);
    assert_eq!(tail.load(Ordering::SeqCst), 1);
    assert_eq!(context.state(), ApplicationState::Incomplete);
}

#[test]
fn test_sync_abort_error_without_wait_is_incomplete() {
    let managed = Managed::synchronous(Arc::new(2_u64), |_| {
        Err(CleanupError::new(std::io::Error::other("cannot stop")))
    });
    let (application, _) = with_dependency(managed, WaitPolicy::unbounded());
    let report = application.begin_shutdown(ShutdownMode::Immediate).abandon();
    assert_eq!(report.incomplete(), [BindingKey::of::<u64>(None)]);
    assert_eq!(report.failures()[0].phase, ShutdownPhase::Abort);
    assert!(!report.is_complete());
}

#[test]
fn test_graceful_request_error_aborts_then_waits_with_termination_budget() {
    let timers = Timers::default();
    let gate = Gate::default();
    let waiting = gate.clone();
    let aborts = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&aborts);
    let managed = Managed::asynchronous(
        Arc::new(2_u64),
        move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        },
        move |_| {
            Box::pin(async move {
                waiting.await;
                Ok(())
            })
        },
    )
    .with_graceful_stop(|_| Err(CleanupError::new(std::io::Error::other("request rejected"))));
    let (application, tail) = with_dependency(managed, timers.policy());
    let mut handle = application.begin_shutdown(ShutdownMode::Graceful);
    assert!(poll_once(handle.wait()).is_pending());
    assert_eq!(timers.durations(), [Duration::from_secs(7)]);
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
    gate.trigger();
    let error = ready(handle.wait()).expect_err("request failure preserved");
    assert_eq!(error.report().failures()[0].phase, ShutdownPhase::RequestGraceful);
    assert!(error.report().is_complete());
    assert!(error.report().fallbacks().is_empty());
    assert_eq!(tail.load(Ordering::SeqCst), 2);
}

#[test]
fn test_graceful_request_panic_is_reported_and_abort_wait_continues() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&aborts);
    let managed = Managed::asynchronous(
        Arc::new(2_u64),
        move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        },
        |_| Box::pin(async { Ok(()) }),
    )
    .with_graceful_stop(|_| panic!("graceful request panic"));
    let (application, tail) = with_dependency(managed, WaitPolicy::unbounded());
    let mut handle = application.begin_shutdown(ShutdownMode::Graceful);
    let error = ready(handle.wait()).expect_err("panic report");
    assert_eq!(error.report().failures()[0].phase, ShutdownPhase::RequestGraceful);
    assert!(error.report().is_complete());
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
    assert_eq!(tail.load(Ordering::SeqCst), 2);
}

#[test]
fn test_abort_panic_does_not_prevent_other_abort_requests_or_waits() {
    let managed = Managed::asynchronous(
        Arc::new(2_u64),
        |_| panic!("abort panic"),
        |_| Box::pin(async { Ok(()) }),
    );
    let (application, tail) = with_dependency(managed, WaitPolicy::unbounded());
    let mut handle = application.begin_shutdown(ShutdownMode::Immediate);
    assert_eq!(tail.load(Ordering::SeqCst), 1);
    let error = ready(handle.wait()).expect_err("panic report");
    assert_eq!(error.report().failures()[0].phase, ShutdownPhase::Abort);
    assert!(error.report().is_complete());
}

/// Checks one-shot wait failures share the same abort and incomplete contract.
fn assert_wait_failure(creating: bool, panicking: bool) {
    let starts = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&starts);
    let aborts = Arc::new(AtomicUsize::new(0));
    let aborted = Arc::clone(&aborts);
    let managed = Managed::asynchronous(
        Arc::new(2_u64),
        move |_| {
            aborted.fetch_add(1, Ordering::SeqCst);
            Ok(())
        },
        move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            assert!(!creating, "wait creation panic");
            Box::pin(async move {
                assert!(!panicking, "wait poll panic");
                Err(CleanupError::new(std::io::Error::other("wait failed")))
            })
        },
    )
    .with_graceful_stop(|_| Ok(()));
    let (application, tail) = with_dependency(managed, WaitPolicy::unbounded());
    let mut handle = application.begin_shutdown(ShutdownMode::Graceful);
    let error = ready(handle.wait()).expect_err("wait failure");
    assert_eq!(error.report().failures()[0].phase, ShutdownPhase::Wait);
    assert_eq!(error.report().incomplete(), [BindingKey::of::<u64>(None)]);
    assert_eq!(tail.load(Ordering::SeqCst), 2);
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
    assert!(ready(handle.wait()).is_err());
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

#[test]
fn test_wait_creation_panic_aborts_marks_incomplete_and_continues() {
    assert_wait_failure(true, false);
}

#[test]
fn test_wait_poll_panic_aborts_marks_incomplete_and_continues() {
    assert_wait_failure(false, true);
}

#[test]
fn test_wait_error_aborts_marks_incomplete_and_never_recreates_wait() {
    assert_wait_failure(false, false);
}

/// Checks deadline creation and polling failures terminate traversal safely.
fn assert_deadline_panic(creating: bool) {
    let policy = WaitPolicy::bounded(Duration::from_secs(13), Duration::from_secs(7), move |_| {
        assert!(!creating, "timer creation panic");
        Box::pin(poll_fn(|_| panic!("timer poll panic")))
    });
    let aborts = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&aborts);
    let gate = Gate::default();
    let managed = Managed::asynchronous(
        Arc::new(2_u64),
        move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        },
        move |_| {
            Box::pin(async move {
                gate.await;
                Ok(())
            })
        },
    )
    .with_graceful_stop(|_| Ok(()));
    let (application, tail) = with_dependency(managed, policy);
    let mut handle = application.begin_shutdown(ShutdownMode::Graceful);
    let error = ready(handle.wait()).expect_err("deadline panic");
    assert_eq!(error.report().failures()[0].phase, ShutdownPhase::Deadline);
    assert_eq!(error.report().incomplete(), [BindingKey::of::<u64>(None)]);
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
    assert_eq!(tail.load(Ordering::SeqCst), 2);
}

#[test]
fn test_timer_creation_panic_aborts_marks_incomplete_and_continues() {
    assert_deadline_panic(true);
}

#[test]
fn test_timer_poll_panic_aborts_marks_incomplete_and_continues() {
    assert_deadline_panic(false);
}
