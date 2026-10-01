// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod support;

use std::error::Error;
use std::future::Future;
use std::io;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::time::Duration;

use qubit_ioc::BindingKey;
use qubit_ioc::CleanupError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Managed;
use qubit_ioc::ShutdownMode;
use qubit_ioc::WaitPolicy;
use support::shutdown_timer::Gate;
use support::shutdown_timer::Timers;
use support::shutdown_timer::poll_once;
use support::shutdown_timer::ready;

/// Adds a pending managed component while recording its abort request.
fn add_pending<T>(builder: &mut ContainerBuilder, aborts: Arc<AtomicUsize>)
where
    T: Default + Send + Sync + 'static,
{
    builder
        .register_managed_factory::<T, _>(&[], move |_| {
            Ok(Managed::new(Arc::new(T::default()), move |_| {
                aborts.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .with_graceful_stop(|_| Ok(()))
            .with_wait(|_| Box::pin(std::future::pending())))
        })
        .expect("register managed component");
}

#[test]
fn test_overall_timeout_aborts_all_pending_entries_and_is_reported() {
    let timers = Timers::default();
    let first_aborts = Arc::new(AtomicUsize::new(0));
    let second_aborts = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().wait_policy(timers.policy_with_total(Duration::from_secs(30)));
    add_pending::<u32>(&mut builder, Arc::clone(&first_aborts));
    add_pending::<u64>(&mut builder, Arc::clone(&second_aborts));
    let application = builder.build_all().expect("build pending components");
    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);

    assert!(poll_once(shutdown.wait()).is_pending());
    assert_eq!(timers.durations(), [Duration::from_secs(30), Duration::from_secs(13)]);
    timers.trigger(0);
    let error = ready(shutdown.wait()).expect_err("overall deadline expires");

    assert_eq!(
        error.report().overall_failure().unwrap().to_string(),
        "component cleanup failed: overall shutdown deadline expired"
    );
    let source = Error::source(error.report().overall_failure().unwrap()).expect("deadline source");
    assert_eq!(
        source.downcast_ref::<io::Error>().unwrap().kind(),
        io::ErrorKind::TimedOut
    );
    assert!(!error.report().is_success());
    assert_eq!(
        error.report().incomplete(),
        [BindingKey::of::<u64>(None), BindingKey::of::<u32>(None)]
    );
    assert_eq!(second_aborts.load(Ordering::SeqCst), 1);
    assert_eq!(first_aborts.load(Ordering::SeqCst), 1);
    assert_eq!(timers.durations(), [Duration::from_secs(30), Duration::from_secs(13)]);
}

#[test]
fn test_cancelling_wait_does_not_restart_overall_deadline() {
    let timers = Timers::default();
    let component_wait = Gate::default();
    let observed_wait = component_wait.clone();
    let mut builder = ContainerBuilder::new().wait_policy(timers.policy_with_total(Duration::from_secs(30)));
    builder
        .register_managed_factory::<u32, _>(&[], move |_| {
            Ok(Managed::new(Arc::new(1), |_| Ok(()))
                .with_graceful_stop(|_| Ok(()))
                .with_wait(move |_| {
                    Box::pin(async move {
                        observed_wait.await;
                        Ok(())
                    })
                }))
        })
        .expect("register managed component");
    let application = builder.build_all().expect("build component");
    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);

    assert!(poll_once(shutdown.wait()).is_pending());
    assert!(poll_once(shutdown.wait()).is_pending());
    assert_eq!(timers.durations(), [Duration::from_secs(30), Duration::from_secs(13)]);
    component_wait.trigger();
    assert!(ready(shutdown.wait()).expect("resumed wait").is_success());
    assert!(
        ready(shutdown.wait())
            .expect("completed wait can be observed again")
            .is_success()
    );
    assert_eq!(timers.durations(), [Duration::from_secs(30), Duration::from_secs(13)]);
}

#[test]
fn test_abort_upgrade_does_not_restart_overall_deadline() {
    let timers = Timers::default();
    let aborts = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().wait_policy(timers.policy_with_total(Duration::from_secs(30)));
    add_pending::<u32>(&mut builder, Arc::clone(&aborts));
    let application = builder.build_all().expect("build pending component");
    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);

    assert!(poll_once(shutdown.wait()).is_pending());
    shutdown.abort();
    assert!(poll_once(shutdown.wait()).is_pending());
    assert_eq!(
        timers.durations(),
        [Duration::from_secs(30), Duration::from_secs(13), Duration::from_secs(7)]
    );

    timers.trigger(0);
    let error = ready(shutdown.wait()).expect_err("overall deadline still applies after abort upgrade");
    assert!(error.report().overall_failure().is_some());
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
}

#[test]
fn test_component_completion_wins_when_overall_deadline_is_also_ready() {
    let timers = Timers::default();
    let component_wait = Gate::default();
    let observed_wait = component_wait.clone();
    let mut builder = ContainerBuilder::new().wait_policy(timers.policy_with_total(Duration::from_secs(30)));
    builder
        .register_managed_factory::<u32, _>(&[], move |_| {
            Ok(Managed::new(Arc::new(1), |_| Ok(()))
                .with_graceful_stop(|_| Ok(()))
                .with_wait(move |_| {
                    Box::pin(async move {
                        observed_wait.await;
                        Ok(())
                    })
                }))
        })
        .expect("register managed component");
    let application = builder.build_all().expect("build component");
    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);
    assert!(poll_once(shutdown.wait()).is_pending());
    timers.trigger(0);
    component_wait.trigger();

    let report = ready(shutdown.wait()).expect("component completion wins");
    assert!(report.is_success());
    assert!(report.overall_failure().is_none());
}

#[test]
fn test_overall_timer_factory_panic_is_reported_and_aborts_entries() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let observed_aborts = Arc::clone(&aborts);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::bounded_with_total(
        Duration::from_secs(13),
        Duration::from_secs(7),
        Duration::from_secs(30),
        move |duration| {
            if duration == Duration::from_secs(30) {
                panic!("overall timer factory panicked");
            }
            Box::pin(std::future::pending())
        },
    ));
    builder
        .register_managed_factory::<u32, _>(&[], move |_| {
            Ok(Managed::new(Arc::new(1), move |_| {
                observed_aborts.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }))
        })
        .expect("register managed component");
    let application = builder.build_all().expect("build component");

    let error = ready(application.begin_shutdown(ShutdownMode::Graceful).wait())
        .expect_err("overall timer factory panic is reported");

    assert!(
        error
            .report()
            .overall_failure()
            .unwrap()
            .to_string()
            .contains("overall deadline factory panicked")
    );
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
}

#[test]
fn test_empty_application_does_not_create_overall_timer() {
    let timers = Timers::default();
    let application = ContainerBuilder::new()
        .wait_policy(timers.policy_with_total(Duration::from_secs(30)))
        .build_all()
        .expect("build empty application");

    let report =
        ready(application.begin_shutdown(ShutdownMode::Graceful).wait()).expect("empty application shuts down");

    assert!(report.is_success());
    assert!(timers.durations().is_empty());
}

#[test]
fn test_overall_timer_poll_panic_is_reported() {
    struct PanicTimer;
    impl Future for PanicTimer {
        type Output = ();
        fn poll(self: std::pin::Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<()> {
            panic!("overall timer poll panicked");
        }
    }

    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::bounded_with_total(
        Duration::from_secs(13),
        Duration::from_secs(7),
        Duration::from_secs(30),
        |_| Box::pin(PanicTimer),
    ));
    builder
        .register_managed_factory::<u32, _>(&[], |_| {
            Ok(Managed::new(Arc::new(1), |_| Ok(()))
                .with_graceful_stop(|_| Ok(()))
                .with_wait(|_| Box::pin(std::future::pending::<Result<(), CleanupError>>())))
        })
        .expect("register managed component");
    let application = builder.build_all().expect("build component");
    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);

    let error = ready(shutdown.wait()).expect_err("overall timer poll panic is reported");
    assert!(
        error
            .report()
            .overall_failure()
            .unwrap()
            .to_string()
            .contains("overall deadline future panicked")
    );
    assert_eq!(error.report().incomplete().len(), 1);
}

#[test]
fn test_existing_bounded_policy_has_no_overall_timer() {
    let timers = Timers::default();
    let mut builder = ContainerBuilder::new().wait_policy(timers.policy());
    builder
        .register_managed_factory::<u32, _>(&[], |_| {
            Ok(Managed::new(Arc::new(1), |_| Ok(())).with_wait(|_| Box::pin(std::future::pending())))
        })
        .expect("register managed component");
    let application = builder.build_all().expect("build component");
    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);

    assert!(poll_once(shutdown.wait()).is_pending());
    assert_eq!(timers.durations(), [Duration::from_secs(7)]);
}

#[test]
fn test_abandon_does_not_create_overall_timer() {
    let timers = Timers::default();
    let mut builder = ContainerBuilder::new().wait_policy(timers.policy_with_total(Duration::from_secs(30)));
    builder
        .register_managed_factory::<u32, _>(&[], |_| Ok(Managed::new(Arc::new(1), |_| Ok(()))))
        .expect("register managed component");
    let application = builder.build_all().expect("build component");

    let report = application.begin_shutdown(ShutdownMode::Graceful).abandon();

    assert!(report.is_complete());
    assert!(timers.durations().is_empty());
}

#[test]
fn test_zero_overall_deadline_aborts_before_starting_component_wait() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let observed_aborts = Arc::clone(&aborts);
    let waits = Arc::new(AtomicUsize::new(0));
    let observed_waits = Arc::clone(&waits);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::bounded_with_total(
        Duration::from_secs(13),
        Duration::from_secs(7),
        Duration::ZERO,
        |_| Box::pin(async {}),
    ));
    builder
        .register_managed_factory::<u32, _>(&[], move |_| {
            Ok(Managed::new(Arc::new(1), move |_| {
                observed_aborts.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .with_graceful_stop(|_| Ok(()))
            .with_wait(move |_| {
                observed_waits.fetch_add(1, Ordering::SeqCst);
                Box::pin(std::future::pending::<Result<(), CleanupError>>())
            }))
        })
        .expect("register managed component");
    let application = builder.build_all().expect("build component");

    let error = ready(application.begin_shutdown(ShutdownMode::Graceful).wait())
        .expect_err("zero overall deadline expires immediately");

    assert_eq!(waits.load(Ordering::SeqCst), 0);
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
    assert_eq!(error.report().incomplete().len(), 1);
}
