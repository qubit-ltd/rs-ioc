// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod support;

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use qubit_ioc::Application;
use qubit_ioc::BindingKey;
use qubit_ioc::BuildError;
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

/// Creates one service with independently counted abort and wait creation
/// actions.
fn waiting(policy: WaitPolicy) -> (Application, Gate, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let gate = Gate::default();
    let waiting = gate.clone();
    let aborts = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&aborts);
    let starts = Arc::new(AtomicUsize::new(0));
    let created = Arc::clone(&starts);
    let mut builder = ContainerBuilder::new().wait_policy(policy);
    builder
        .register_managed_factory::<u32, _>(&[], move |_| {
            Ok(Managed::asynchronous_with_graceful(
                Arc::new(1),
                move |_| {
                    observed.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
                |_| Ok(()),
                move |_| {
                    created.fetch_add(1, Ordering::SeqCst);
                    Box::pin(async move {
                        waiting.await;
                        Ok(())
                    })
                },
            ))
        })
        .expect("register waiting service");
    (
        builder.build_all().expect("build waiting service"),
        gate,
        aborts,
        starts,
    )
}

#[test]
fn test_missing_policy_is_rejected_before_any_selected_factory_runs() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let mut builder = ContainerBuilder::new();
    builder
        .register_managed_factory::<u32, _>(&[], move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(Managed::synchronous(Arc::new(1), |_| Ok(())))
        })
        .expect("register managed service");
    assert!(matches!(builder.build_all(), Err(failure) if matches!(failure.cause(), BuildError::MissingWaitPolicy)));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn test_nonmanaged_graph_needs_no_wait_policy() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<u32, _>(&[], |_| Ok(Arc::new(7)))
        .expect("register ordinary service");
    let application = builder
        .build_all()
        .expect("ordinary graph has no wait policy requirement");
    assert_eq!(*application.context().get::<u32>().expect("u32 service"), 7);
}

#[test]
fn test_unselected_managed_factory_needs_no_wait_policy() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<u32, _>(&[], |_| Ok(Arc::new(7)))
        .expect("register ordinary root");
    builder
        .register_managed_factory::<u64, _>(&[], |_| panic!("unselected factory must not run"))
        .expect("register unselected managed service");
    builder.root::<u32>();
    assert!(builder.build().is_ok());
}

#[test]
fn test_unbounded_wait_can_resume_without_deadlines() {
    let (application, gate, aborts, starts) = waiting(WaitPolicy::unbounded());
    let mut handle = application.begin_shutdown(ShutdownMode::Graceful);
    assert!(poll_once(handle.wait()).is_pending());
    assert!(poll_once(handle.wait()).is_pending());
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    gate.trigger();
    assert!(ready(handle.wait()).expect("wait completes").is_success());
    assert_eq!(aborts.load(Ordering::SeqCst), 0);
}

#[test]
fn test_cancel_and_resume_reuses_wait_and_grace_deadline() {
    let timers = Timers::default();
    let (application, gate, aborts, starts) = waiting(timers.policy());
    let mut handle = application.begin_shutdown(ShutdownMode::Graceful);
    assert!(poll_once(handle.wait()).is_pending());
    assert!(poll_once(handle.wait()).is_pending());
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert_eq!(timers.durations(), [Duration::from_secs(13)]);
    gate.trigger();
    assert!(ready(handle.wait()).expect("resumed wait").is_success());
    assert_eq!(aborts.load(Ordering::SeqCst), 0);
}

#[test]
fn test_grace_timeout_preserves_wait_and_installs_termination_once() {
    let timers = Timers::default();
    let (application, gate, aborts, starts) = waiting(timers.policy());
    let mut handle = application.begin_shutdown(ShutdownMode::Graceful);
    assert!(poll_once(handle.wait()).is_pending());
    timers.trigger(0);
    assert!(poll_once(handle.wait()).is_pending());
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
    assert_eq!(timers.durations(), [Duration::from_secs(13), Duration::from_secs(7)]);
    assert!(poll_once(handle.wait()).is_pending());
    gate.trigger();
    let error = ready(handle.wait()).expect_err("grace deadline retained in report");
    assert_eq!(error.report().failures()[0].phase, ShutdownPhase::GracefulWait);
    assert!(error.report().incomplete().is_empty());
    assert!(error.report().is_complete());
    assert!(!error.report().is_success());
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert_eq!(timers.durations().len(), 2);
}

#[test]
fn test_explicit_abort_preserves_active_wait_and_does_not_reset_termination_budget() {
    let timers = Timers::default();
    let (application, _, aborts, starts) = waiting(timers.policy());
    let mut handle = application.begin_shutdown(ShutdownMode::Graceful);
    assert!(poll_once(handle.wait()).is_pending());
    handle.abort();
    assert!(poll_once(handle.wait()).is_pending());
    handle.abort();
    assert!(poll_once(handle.wait()).is_pending());
    assert_eq!(timers.durations(), [Duration::from_secs(13), Duration::from_secs(7)]);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
    timers.trigger(1);
    let error = ready(handle.wait()).expect_err("termination deadline");
    assert_eq!(error.report().failures()[0].phase, ShutdownPhase::TerminationWait);
    assert_eq!(error.report().incomplete(), [BindingKey::of::<u32>(None)]);
}

#[test]
fn test_abort_after_grace_timeout_does_not_create_another_termination_timer() {
    let timers = Timers::default();
    let (application, _, aborts, _) = waiting(timers.policy());
    let mut handle = application.begin_shutdown(ShutdownMode::Graceful);
    assert!(poll_once(handle.wait()).is_pending());
    timers.trigger(0);
    assert!(poll_once(handle.wait()).is_pending());
    handle.abort();
    handle.abort();
    assert!(poll_once(handle.wait()).is_pending());
    assert_eq!(timers.durations().len(), 2);
    timers.trigger(1);
    let error = ready(handle.wait()).expect_err("both deadlines");
    let phases: Vec<_> = error.report().failures().iter().map(|failure| failure.phase).collect();
    assert_eq!(phases, [ShutdownPhase::GracefulWait, ShutdownPhase::TerminationWait]);
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
}

#[test]
fn test_zero_deadline_ready_wait_has_priority() {
    let policy = WaitPolicy::bounded(Duration::ZERO, Duration::ZERO, |_| Box::pin(async {}));
    let (application, gate, aborts, _) = waiting(policy);
    gate.trigger();
    let mut handle = application.begin_shutdown(ShutdownMode::Graceful);
    assert!(
        ready(handle.wait())
            .expect("ready wait beats ready deadline")
            .is_success()
    );
    assert_eq!(aborts.load(Ordering::SeqCst), 0);
}

#[test]
fn test_immediate_uses_termination_budget_and_preserves_it_after_cancellation() {
    let timers = Timers::default();
    let (application, gate, aborts, starts) = waiting(timers.policy());
    let mut handle = application.begin_shutdown(ShutdownMode::Immediate);
    assert!(poll_once(handle.wait()).is_pending());
    handle.abort();
    assert!(poll_once(handle.wait()).is_pending());
    assert_eq!(timers.durations(), [Duration::from_secs(7)]);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    gate.trigger();
    assert!(ready(handle.wait()).expect("immediate completes").is_success());
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
}

#[test]
fn test_each_entry_receives_an_independent_termination_deadline() {
    let timers = Timers::default();
    let first = Gate::default();
    let second = Gate::default();
    let waiting_first = first.clone();
    let waiting_second = second.clone();
    let mut builder = ContainerBuilder::new().wait_policy(timers.policy());
    builder
        .register_managed_factory::<u32, _>(&[], move |_| {
            Ok(Managed::asynchronous(
                Arc::new(1),
                |_| Ok(()),
                move |_| {
                    Box::pin(async move {
                        waiting_first.await;
                        Ok(())
                    })
                },
            ))
        })
        .expect("register dependency");
    builder
        .register_managed_factory::<u64, _>(&[Dependency::of::<u32>()], move |_| {
            Ok(Managed::asynchronous(
                Arc::new(2),
                |_| Ok(()),
                move |_| {
                    Box::pin(async move {
                        waiting_second.await;
                        Ok(())
                    })
                },
            ))
        })
        .expect("register consumer");
    let mut handle = builder
        .build_all()
        .expect("build")
        .begin_shutdown(ShutdownMode::Immediate);
    assert!(poll_once(handle.wait()).is_pending());
    timers.trigger(0);
    assert!(
        poll_once(handle.wait()).is_pending(),
        "dependency receives a fresh deadline despite consumer timeout"
    );
    assert_eq!(timers.durations(), [Duration::from_secs(7), Duration::from_secs(7)]);
    first.trigger();
    let error = ready(handle.wait()).expect_err("consumer remains incomplete");
    assert_eq!(error.report().incomplete(), [BindingKey::of::<u64>(None)]);
    assert_eq!(error.report().failures().len(), 1);
}

#[test]
fn test_graceful_fallback_uses_only_termination_deadline() {
    let timers = Timers::default();
    let gate = Gate::default();
    let waiting = gate.clone();
    let mut builder = ContainerBuilder::new().wait_policy(timers.policy());
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
            ))
        })
        .expect("register fallback service");
    let mut handle = builder
        .build_all()
        .expect("build")
        .begin_shutdown(ShutdownMode::Graceful);
    assert!(poll_once(handle.wait()).is_pending());
    assert_eq!(timers.durations(), [Duration::from_secs(7)]);
    gate.trigger();
    let report = ready(handle.wait()).expect("fallback success");
    assert_eq!(report.fallbacks(), [BindingKey::of::<u32>(None)]);
    assert!(report.is_success());
}
