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

use qubit_ioc::ApplicationState;
use qubit_ioc::BindingId;
use qubit_ioc::BindingKey;
use qubit_ioc::BindingOptions;
use qubit_ioc::CleanupError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Managed;
use qubit_ioc::ShutdownMode;
use qubit_ioc::ShutdownPhase;
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
            Ok(Managed::asynchronous_with_graceful(
                Arc::new(T::default()),
                move |_| {
                    aborts.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
                |_| Ok(()),
                |_| Box::pin(std::future::pending()),
            ))
        })
        .expect("register managed component");
}

fn add_synchronous_resources(
    builder: &mut ContainerBuilder,
    count: usize,
    stops: Arc<AtomicUsize>,
    fail_at: Option<usize>,
) {
    for index in 0..count {
        let stops = Arc::clone(&stops);
        builder
            .register_managed_factory_with::<usize, _>(
                &[],
                BindingOptions {
                    id: Some(format!("resource.n{index}")),
                    ..BindingOptions::default()
                },
                move |_| {
                    Ok(Managed::synchronous(Arc::new(index), move |_| {
                        stops.fetch_add(1, Ordering::SeqCst);
                        if fail_at == Some(index) {
                            Err(CleanupError::new(io::Error::other("expected abort failure")))
                        } else {
                            Ok(())
                        }
                    }))
                },
            )
            .expect("register synchronous component");
    }
}

#[test]
fn test_immediate_shutdown_across_batches_does_not_create_overall_timer() {
    for count in [63, 64, 65, 129] {
        for timer_panics in [false, true] {
            let timer_calls = Arc::new(AtomicUsize::new(0));
            let observed_timer_calls = Arc::clone(&timer_calls);
            let stops = Arc::new(AtomicUsize::new(0));
            let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::bounded_with_total(
                Duration::from_secs(13),
                Duration::from_secs(7),
                Duration::from_secs(30),
                move |_| {
                    observed_timer_calls.fetch_add(1, Ordering::SeqCst);
                    if timer_panics {
                        panic!("overall timer must not be created");
                    }
                    Box::pin(std::future::ready(()))
                },
            ));
            add_synchronous_resources(&mut builder, count, Arc::clone(&stops), None);
            let application = builder.build_all().expect("build components");
            let context = application.context().clone();
            let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);

            assert!(shutdown.pending().is_empty());
            let report = (0..count + 4)
                .find_map(|_| match poll_once(shutdown.wait()) {
                    Poll::Ready(result) => Some(result),
                    Poll::Pending => None,
                })
                .expect("shutdown completes within bounded polls")
                .expect("completed shutdown succeeds");
            assert!(report.overall_failure().is_none());
            assert_eq!(timer_calls.load(Ordering::SeqCst), 0);
            assert_eq!(stops.load(Ordering::SeqCst), count);
            assert_eq!(context.state(), ApplicationState::Closed);
            assert!(ready(shutdown.wait()).expect("repeat wait succeeds").is_success());
            assert_eq!(stops.load(Ordering::SeqCst), count);
        }
    }
}

#[test]
fn test_immediate_shutdown_abort_failure_across_batches_does_not_create_overall_failure() {
    const COUNT: usize = 129;
    let timer_calls = Arc::new(AtomicUsize::new(0));
    let observed_timer_calls = Arc::clone(&timer_calls);
    let stops = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::bounded_with_total(
        Duration::from_secs(13),
        Duration::from_secs(7),
        Duration::from_secs(30),
        move |_| {
            observed_timer_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(std::future::ready(()))
        },
    ));
    add_synchronous_resources(&mut builder, COUNT, Arc::clone(&stops), Some(64));
    let application = builder.build_all().expect("build components");
    let context = application.context().clone();
    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);

    let error = (0..COUNT + 4)
        .find_map(|_| match poll_once(shutdown.wait()) {
            Poll::Ready(result) => Some(result),
            Poll::Pending => None,
        })
        .expect("shutdown completes within bounded polls")
        .expect_err("abort failure is retained");
    assert!(error.report().overall_failure().is_none());
    assert_eq!(
        error.report().incomplete(),
        [BindingKey::of::<usize>(Some(BindingId::parse("resource.n64").unwrap()))]
    );
    assert_eq!(error.report().failures().len(), 1);
    assert_eq!(
        error.report().failures()[0]
            .error
            .source()
            .expect("original abort error")
            .to_string(),
        "expected abort failure"
    );
    assert_eq!(context.state(), ApplicationState::Incomplete);
    assert_eq!(timer_calls.load(Ordering::SeqCst), 0);
    assert_eq!(stops.load(Ordering::SeqCst), COUNT);
}

#[test]
fn test_graceful_abort_before_first_wait_skips_overall_timer_when_all_work_is_done() {
    const COUNT: usize = 65;
    let timers = Timers::default();
    let stops = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().wait_policy(timers.policy_with_total(Duration::from_secs(30)));
    add_synchronous_resources(&mut builder, COUNT, Arc::clone(&stops), None);
    let application = builder.build_all().expect("build components");
    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);

    shutdown.abort();
    assert!(shutdown.pending().is_empty());
    let report = (0..COUNT + 4)
        .find_map(|_| match poll_once(shutdown.wait()) {
            Poll::Ready(result) => Some(result),
            Poll::Pending => None,
        })
        .expect("shutdown completes within bounded polls")
        .expect("aborted shutdown succeeds");
    assert!(report.overall_failure().is_none());
    assert_eq!(timers.durations(), []);
    assert_eq!(stops.load(Ordering::SeqCst), COUNT);
}

#[test]
fn test_abort_restore_deadline_panic_does_not_restart_overall_timer() {
    let overall_calls = Arc::new(AtomicUsize::new(0));
    let observed_overall_calls = Arc::clone(&overall_calls);
    let termination_calls = Arc::new(AtomicUsize::new(0));
    let observed_termination_calls = Arc::clone(&termination_calls);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::bounded_with_total(
        Duration::from_secs(13),
        Duration::from_secs(7),
        Duration::from_secs(30),
        move |duration| {
            if duration == Duration::from_secs(30) {
                observed_overall_calls.fetch_add(1, Ordering::SeqCst);
            }
            if duration == Duration::from_secs(7) {
                observed_termination_calls.fetch_add(1, Ordering::SeqCst);
                panic!("termination deadline factory panicked");
            }
            Box::pin(std::future::pending())
        },
    ));
    builder
        .register_managed_factory::<u32, _>(&[], |_| {
            Ok(Managed::asynchronous_with_graceful(
                Arc::new(1),
                |_| Ok(()),
                |_| Ok(()),
                |_| Box::pin(std::future::pending()),
            ))
        })
        .expect("register managed component");
    let application = builder.build_all().expect("build component");
    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);

    assert!(poll_once(shutdown.wait()).is_pending());
    assert_eq!(overall_calls.load(Ordering::SeqCst), 1);
    shutdown.abort();
    let error = ready(shutdown.wait()).expect_err("termination deadline panic is reported");

    assert!(error.report().overall_failure().is_none());
    assert_eq!(error.report().incomplete(), [BindingKey::of::<u32>(None)]);
    assert_eq!(error.report().failures().len(), 1);
    assert_eq!(error.report().failures()[0].phase, ShutdownPhase::Deadline);
    assert_eq!(overall_calls.load(Ordering::SeqCst), 1);
    assert_eq!(termination_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn test_pending_work_after_terminal_batch_still_starts_overall_timer_on_first_poll() {
    const COUNT: usize = 65;
    let timers = Timers::default();
    let async_aborts = Arc::new(AtomicUsize::new(0));
    let sync_stops = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().wait_policy(timers.policy_with_total(Duration::from_secs(30)));
    add_pending::<u32>(&mut builder, Arc::clone(&async_aborts));
    add_synchronous_resources(&mut builder, COUNT, Arc::clone(&sync_stops), None);
    let application = builder.build_all().expect("build mixed components");
    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);

    assert!(poll_once(shutdown.wait()).is_pending());
    assert_eq!(timers.durations(), [Duration::from_secs(30)]);
    assert!(poll_once(shutdown.wait()).is_pending());
    assert_eq!(timers.durations(), [Duration::from_secs(30), Duration::from_secs(7)]);
    timers.trigger(0);
    let error = ready(shutdown.wait()).expect_err("overall deadline expires");
    assert!(error.report().overall_failure().is_some());
    assert_eq!(error.report().incomplete(), [BindingKey::of::<u32>(None)]);
    assert_eq!(async_aborts.load(Ordering::SeqCst), 1);
    assert_eq!(sync_stops.load(Ordering::SeqCst), COUNT);
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
    assert_eq!(shutdown.pending().as_slice(), error.report().incomplete());
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
            Ok(Managed::asynchronous_with_graceful(
                Arc::new(1),
                |_| Ok(()),
                |_| Ok(()),
                move |_| {
                    Box::pin(async move {
                        observed_wait.await;
                        Ok(())
                    })
                },
            ))
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
            Ok(Managed::asynchronous_with_graceful(
                Arc::new(1),
                |_| Ok(()),
                |_| Ok(()),
                move |_| {
                    Box::pin(async move {
                        observed_wait.await;
                        Ok(())
                    })
                },
            ))
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
            Ok(Managed::synchronous(Arc::new(1), move |_| {
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
fn test_immediate_sync_completion_does_not_start_overall_timer() {
    let timers = Arc::new(AtomicUsize::new(0));
    let timer_calls = Arc::clone(&timers);
    let stops = Arc::new(AtomicUsize::new(0));
    let stop_calls = Arc::clone(&stops);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::bounded_with_total(
        Duration::from_secs(13),
        Duration::from_secs(7),
        Duration::ZERO,
        move |_| {
            timer_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {})
        },
    ));
    builder
        .register_managed_factory::<u32, _>(&[], move |_| {
            Ok(Managed::synchronous(Arc::new(1), move |_| {
                stop_calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }))
        })
        .expect("register synchronous managed component");
    let application = builder.build_all().expect("build component");
    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);
    assert!(shutdown.pending().is_empty());
    let report = ready(shutdown.wait()).expect("already completed shutdown");
    assert!(report.is_success());
    assert!(report.overall_failure().is_none());
    assert_eq!(timers.load(Ordering::SeqCst), 0);
    assert_eq!(stops.load(Ordering::SeqCst), 1);
    assert!(ready(shutdown.wait()).expect("repeat observation").is_success());
    assert_eq!(stops.load(Ordering::SeqCst), 1);
}

#[test]
fn test_immediate_sync_completion_skips_panicking_overall_timer() {
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::bounded_with_total(
        Duration::from_secs(13),
        Duration::from_secs(7),
        Duration::ZERO,
        |_| panic!("completed shutdown must not create a timer"),
    ));
    builder
        .register_managed_factory::<u32, _>(&[], |_| Ok(Managed::synchronous(Arc::new(1), |_| Ok(()))))
        .expect("register synchronous managed component");
    let application = builder.build_all().expect("build component");

    let report = ready(application.begin_shutdown(ShutdownMode::Immediate).wait()).expect("already completed shutdown");
    assert!(report.is_success());
    assert!(report.overall_failure().is_none());
}

#[test]
fn test_immediate_sync_failure_preserves_report_without_timer() {
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::bounded_with_total(
        Duration::from_secs(13),
        Duration::from_secs(7),
        Duration::ZERO,
        |_| panic!("completed shutdown must not create a timer"),
    ));
    builder
        .register_managed_factory::<u32, _>(&[], |_| {
            Ok(Managed::synchronous(Arc::new(1), |_| {
                Err(CleanupError::new(io::Error::other("sync stop failed")))
            }))
        })
        .expect("register synchronous managed component");
    let application = builder.build_all().expect("build component");
    let context = application.context().clone();

    let error = ready(application.begin_shutdown(ShutdownMode::Immediate).wait())
        .expect_err("synchronous cleanup failure is reported");
    let report = error.report();
    assert_eq!(report.failures().len(), 1);
    assert_eq!(report.failures()[0].phase, ShutdownPhase::Abort);
    let source = report.failures()[0]
        .error
        .source()
        .expect("original cleanup source")
        .downcast_ref::<io::Error>()
        .expect("io error remains downcastable");
    assert_eq!(source.to_string(), "sync stop failed");
    assert!(report.overall_failure().is_none());
    assert_eq!(report.incomplete(), [BindingKey::of::<u32>(None)]);
    assert_eq!(context.state(), ApplicationState::Incomplete);
}

#[test]
fn test_immediate_mixed_completion_still_uses_overall_timer() {
    let timers = Timers::default();
    let async_aborts = Arc::new(AtomicUsize::new(0));
    let sync_stops = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().wait_policy(timers.policy_with_total(Duration::from_secs(30)));
    add_pending::<u32>(&mut builder, Arc::clone(&async_aborts));
    let sync_calls = Arc::clone(&sync_stops);
    builder
        .register_managed_factory::<u64, _>(&[], move |_| {
            Ok(Managed::synchronous(Arc::new(1), move |_| {
                sync_calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }))
        })
        .expect("register synchronous managed component");
    let application = builder.build_all().expect("build mixed components");
    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);
    assert_eq!(shutdown.pending(), &[BindingKey::of::<u32>(None)]);

    assert!(poll_once(shutdown.wait()).is_pending());
    assert_eq!(timers.durations(), [Duration::from_secs(30), Duration::from_secs(7)]);
    timers.trigger(0);
    let error = ready(shutdown.wait()).expect_err("overall deadline expires");
    assert!(error.report().overall_failure().is_some());
    assert_eq!(error.report().incomplete(), [BindingKey::of::<u32>(None)]);
    assert_eq!(async_aborts.load(Ordering::SeqCst), 1);
    assert_eq!(sync_stops.load(Ordering::SeqCst), 1);
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
            Ok(Managed::asynchronous_with_graceful(
                Arc::new(1),
                |_| Ok(()),
                |_| Ok(()),
                |_| Box::pin(std::future::pending::<Result<(), CleanupError>>()),
            ))
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
            Ok(Managed::asynchronous(
                Arc::new(1),
                |_| Ok(()),
                |_| Box::pin(std::future::pending()),
            ))
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
        .register_managed_factory::<u32, _>(&[], |_| Ok(Managed::synchronous(Arc::new(1), |_| Ok(()))))
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
            Ok(Managed::asynchronous_with_graceful(
                Arc::new(1),
                move |_| {
                    observed_aborts.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
                |_| Ok(()),
                move |_| {
                    observed_waits.fetch_add(1, Ordering::SeqCst);
                    Box::pin(std::future::pending::<Result<(), CleanupError>>())
                },
            ))
        })
        .expect("register managed component");
    let application = builder.build_all().expect("build component");

    let error = ready(application.begin_shutdown(ShutdownMode::Graceful).wait())
        .expect_err("zero overall deadline expires immediately");

    assert_eq!(waits.load(Ordering::SeqCst), 0);
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
    assert_eq!(error.report().incomplete().len(), 1);
}
