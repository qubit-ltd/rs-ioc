// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Cooperative scheduling of immediately completed shutdown observations.

#[allow(dead_code)]
mod support;

use std::future::Future;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Wake;
use std::task::Waker;

use qubit_ioc::BindingOptions;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Managed;
use qubit_ioc::ShutdownMode;
use qubit_ioc::WaitPolicy;
use support::shutdown_timer::Gate;
use support::shutdown_timer::Timers;

struct CountWake(AtomicUsize);

impl Wake for CountWake {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn immediate_shutdown_builder(count: usize, observed: Arc<Mutex<Vec<(char, usize)>>>) -> ContainerBuilder {
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    for index in 0..count {
        let graceful = Arc::clone(&observed);
        let abort = Arc::clone(&observed);
        let wait = Arc::clone(&observed);
        builder
            .register_managed_factory_with::<usize, _>(
                &[],
                BindingOptions {
                    id: Some(format!("resource.n{index}")),
                    ..BindingOptions::default()
                },
                move |_| {
                    Ok(Managed::asynchronous_with_graceful(
                        Arc::new(index),
                        move |_| {
                            abort.lock().expect("log").push(('a', index));
                            Ok(())
                        },
                        move |_| {
                            graceful.lock().expect("log").push(('g', index));
                            Ok(())
                        },
                        move |_| {
                            wait.lock().expect("log").push(('w', index));
                            Box::pin(async { Ok(()) })
                        },
                    ))
                },
            )
            .expect("register component");
    }
    builder
}

fn synchronous_shutdown_builder(count: usize, observed: Arc<Mutex<Vec<usize>>>) -> ContainerBuilder {
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    for index in 0..count {
        let observed = Arc::clone(&observed);
        builder
            .register_managed_factory_with::<usize, _>(
                &[],
                BindingOptions {
                    id: Some(format!("resource.n{index}")),
                    ..BindingOptions::default()
                },
                move |_| {
                    Ok(Managed::synchronous_with_graceful(
                        Arc::new(index),
                        move |_| {
                            observed.lock().expect("log").push(index);
                            Ok(())
                        },
                        |_| Ok(()),
                    ))
                },
            )
            .expect("register component");
    }
    builder
}

#[test]
fn test_immediately_ready_shutdown_yields_and_preserves_reverse_order() {
    const COUNT: usize = 257;

    let observed = Arc::new(Mutex::new(Vec::new()));
    let application = immediate_shutdown_builder(COUNT, Arc::clone(&observed))
        .build_all()
        .expect("build components");
    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);
    let wake_counter = Arc::new(CountWake(AtomicUsize::new(0)));
    let waker = Waker::from(Arc::clone(&wake_counter));
    let mut context = Context::from_waker(&waker);
    let mut wait = Box::pin(shutdown.wait());

    assert!(wait.as_mut().poll(&mut context).is_pending());
    assert!(observed.lock().expect("log").len() < COUNT);
    assert!(wake_counter.0.load(Ordering::SeqCst) > 0);

    let competitor_polled = Arc::new(AtomicUsize::new(0));
    let competitor_seen = Arc::clone(&competitor_polled);
    let mut competitor = Box::pin(std::future::poll_fn(move |_| {
        competitor_seen.fetch_add(1, Ordering::SeqCst);
        Poll::<()>::Ready(())
    }));
    assert!(competitor.as_mut().poll(&mut context).is_ready());
    assert_eq!(competitor_polled.load(Ordering::SeqCst), 1);
    assert!(observed.lock().expect("log").len() < COUNT);

    let mut result = None;
    for _ in 0..16 {
        if let Poll::Ready(value) = wait.as_mut().poll(&mut context) {
            result = Some(value);
            break;
        }
    }
    result
        .expect("bounded polling completes shutdown")
        .expect("shutdown succeeds");
    let expected = (0..COUNT)
        .rev()
        .flat_map(|index| [('g', index), ('w', index)])
        .collect::<Vec<_>>();
    assert_eq!(*observed.lock().expect("log"), expected);
}

#[test]
fn test_batch_skips_done_and_incomplete_entries_cooperatively() {
    const COUNT: usize = 65;

    let aborts = Arc::new(Mutex::new(Vec::new()));
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    for index in 0..COUNT {
        let aborts = Arc::clone(&aborts);
        builder
            .register_managed_factory_with::<usize, _>(
                &[],
                BindingOptions {
                    id: Some(format!("resource.n{index}")),
                    ..BindingOptions::default()
                },
                move |_| {
                    Ok(Managed::synchronous(Arc::new(index), move |_| {
                        aborts.lock().expect("log").push(index);
                        if index == COUNT - 1 {
                            Err(qubit_ioc::CleanupError::new(std::io::Error::other(
                                "expected abort failure",
                            )))
                        } else {
                            Ok(())
                        }
                    }))
                },
            )
            .expect("register component");
    }
    let application = builder.build_all().expect("build components");
    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);
    let wake_counter = Arc::new(CountWake(AtomicUsize::new(0)));
    let waker = Waker::from(Arc::clone(&wake_counter));
    let mut context = Context::from_waker(&waker);
    let mut wait = Box::pin(shutdown.wait());

    assert!(wait.as_mut().poll(&mut context).is_pending());
    assert_eq!(wake_counter.0.load(Ordering::SeqCst), 1);
    assert_eq!(aborts.lock().expect("log").len(), COUNT);
    assert!(wait.as_mut().poll(&mut context).is_ready());
}

#[test]
fn test_wait_error_does_not_stop_later_shutdown_entries() {
    const COUNT: usize = 65;

    let observed = Arc::new(Mutex::new(Vec::new()));
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    for index in 0..COUNT {
        let observed = Arc::clone(&observed);
        builder
            .register_managed_factory_with::<usize, _>(
                &[],
                BindingOptions {
                    id: Some(format!("resource.n{index}")),
                    ..BindingOptions::default()
                },
                move |_| {
                    Ok(Managed::asynchronous_with_graceful(
                        Arc::new(index),
                        |_| Ok(()),
                        |_| Ok(()),
                        move |_| {
                            observed.lock().expect("log").push(index);
                            Box::pin(async move {
                                if index == COUNT - 1 {
                                    Err(qubit_ioc::CleanupError::new(std::io::Error::other(
                                        "expected wait failure",
                                    )))
                                } else {
                                    Ok(())
                                }
                            })
                        },
                    ))
                },
            )
            .expect("register component");
    }
    let application = builder.build_all().expect("build components");
    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);
    let mut wait = Box::pin(shutdown.wait());
    let mut context = Context::from_waker(Waker::noop());
    let mut result = None;
    for _ in 0..16 {
        if let Poll::Ready(value) = wait.as_mut().poll(&mut context) {
            result = Some(value);
            break;
        }
    }
    assert!(result.expect("bounded shutdown completes").is_err());
    assert_eq!(*observed.lock().expect("log"), (0..COUNT).rev().collect::<Vec<_>>());
}

#[test]
fn test_empty_and_batch_boundary_shutdowns_only_yield_when_work_remains() {
    for count in [0, 1, 63, 64, 65] {
        let observed = Arc::new(Mutex::new(Vec::new()));
        let application = synchronous_shutdown_builder(count, Arc::clone(&observed))
            .build_all()
            .expect("build components");
        let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);
        let wake_counter = Arc::new(CountWake(AtomicUsize::new(0)));
        let waker = Waker::from(Arc::clone(&wake_counter));
        let mut context = Context::from_waker(&waker);
        let mut wait = Box::pin(shutdown.wait());
        let poll = wait.as_mut().poll(&mut context);

        if count <= 64 {
            assert!(poll.is_ready(), "{count} entries should finish in one poll");
            assert_eq!(wake_counter.0.load(Ordering::SeqCst), 0);
        } else {
            assert!(poll.is_pending(), "65 entries should yield after the first batch");
            assert_eq!(wake_counter.0.load(Ordering::SeqCst), 1);
            assert!(wait.as_mut().poll(&mut context).is_ready());
        }
        assert_eq!(observed.lock().expect("log").len(), count);
    }
}

#[test]
fn test_cancelling_at_batch_boundary_retains_active_wait_and_deadlines() {
    const SYNC_COUNT: usize = 63;

    let timers = Timers::default();
    let component_wait = Gate::default();
    let wait_gate = component_wait.clone();
    let wait_starts = Arc::new(AtomicUsize::new(0));
    let observed_starts = Arc::clone(&wait_starts);
    let graceful_calls = Arc::new(AtomicUsize::new(0));
    let observed_graceful = Arc::clone(&graceful_calls);
    let abort_calls = Arc::new(AtomicUsize::new(0));
    let observed_abort = Arc::clone(&abort_calls);
    let sync_stops = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().wait_policy(timers.policy_with_total(std::time::Duration::from_secs(30)));
    builder
        .register_managed_factory_with::<usize, _>(
            &[],
            BindingOptions {
                id: Some("resource.active".to_owned()),
                ..BindingOptions::default()
            },
            move |_| {
                let observed_graceful = Arc::clone(&observed_graceful);
                let observed_abort = Arc::clone(&observed_abort);
                let observed_starts = Arc::clone(&observed_starts);
                Ok(Managed::asynchronous_with_graceful(
                    Arc::new(0),
                    move |_| {
                        observed_abort.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    },
                    move |_| {
                        observed_graceful.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    },
                    move |_| {
                        observed_starts.fetch_add(1, Ordering::SeqCst);
                        Box::pin(async move {
                            wait_gate.await;
                            Ok(())
                        })
                    },
                ))
            },
        )
        .expect("register active component");
    for index in 1..=SYNC_COUNT {
        let sync_stops = Arc::clone(&sync_stops);
        builder
            .register_managed_factory_with::<usize, _>(
                &[],
                BindingOptions {
                    id: Some(format!("resource.sync{index}")),
                    ..BindingOptions::default()
                },
                move |_| {
                    Ok(Managed::synchronous_with_graceful(
                        Arc::new(index),
                        move |_| {
                            sync_stops.fetch_add(1, Ordering::SeqCst);
                            Ok(())
                        },
                        |_| Ok(()),
                    ))
                },
            )
            .expect("register synchronous component");
    }

    let application = builder.build_all().expect("build components");
    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);
    let wake_counter = Arc::new(CountWake(AtomicUsize::new(0)));
    let waker = Waker::from(Arc::clone(&wake_counter));
    let mut context = Context::from_waker(&waker);
    let mut borrowed_wait = Box::pin(shutdown.wait());

    assert!(borrowed_wait.as_mut().poll(&mut context).is_pending());
    assert_eq!(wake_counter.0.load(Ordering::SeqCst), 1);
    assert_eq!(sync_stops.load(Ordering::SeqCst), SYNC_COUNT);
    assert_eq!(graceful_calls.load(Ordering::SeqCst), 1);
    assert_eq!(wait_starts.load(Ordering::SeqCst), 1);
    assert_eq!(
        timers.durations(),
        [std::time::Duration::from_secs(30), std::time::Duration::from_secs(13),]
    );
    drop(borrowed_wait);

    let mut resumed_wait = Box::pin(shutdown.wait());
    assert!(resumed_wait.as_mut().poll(&mut context).is_pending());
    assert_eq!(wake_counter.0.load(Ordering::SeqCst), 1);
    assert_eq!(wait_starts.load(Ordering::SeqCst), 1);
    assert_eq!(graceful_calls.load(Ordering::SeqCst), 1);
    assert_eq!(timers.durations().len(), 2);

    drop(resumed_wait);
    shutdown.abort();
    assert_eq!(abort_calls.load(Ordering::SeqCst), 1);
    assert_eq!(wait_starts.load(Ordering::SeqCst), 1);
    assert_eq!(
        timers.durations(),
        [
            std::time::Duration::from_secs(30),
            std::time::Duration::from_secs(13),
            std::time::Duration::from_secs(7),
        ]
    );
    let mut resumed_wait = Box::pin(shutdown.wait());
    assert!(resumed_wait.as_mut().poll(&mut context).is_pending());
    assert_eq!(wake_counter.0.load(Ordering::SeqCst), 1);
    assert_eq!(wait_starts.load(Ordering::SeqCst), 1);
    assert_eq!(abort_calls.load(Ordering::SeqCst), 1);

    component_wait.trigger();
    assert!(resumed_wait.as_mut().poll(&mut context).is_ready());
    assert_eq!(wait_starts.load(Ordering::SeqCst), 1);
    assert_eq!(graceful_calls.load(Ordering::SeqCst), 1);
    assert_eq!(abort_calls.load(Ordering::SeqCst), 1);
    assert_eq!(timers.durations().len(), 3);
}

#[test]
fn test_ready_overall_deadline_wins_at_batch_yield_boundary() {
    const COUNT: usize = 65;

    let timers = Timers::default();
    let trigger_timers = timers.clone();
    let graceful_calls = Arc::new(AtomicUsize::new(0));
    let abort_calls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().wait_policy(timers.policy_with_total(std::time::Duration::from_secs(30)));
    for index in 0..COUNT {
        let trigger_timers = trigger_timers.clone();
        let graceful_calls = Arc::clone(&graceful_calls);
        let abort_calls = Arc::clone(&abort_calls);
        builder
            .register_managed_factory_with::<usize, _>(
                &[],
                BindingOptions {
                    id: Some(format!("resource.n{index}")),
                    ..BindingOptions::default()
                },
                move |_| {
                    Ok(Managed::synchronous_with_graceful(
                        Arc::new(index),
                        move |_| {
                            abort_calls.fetch_add(1, Ordering::SeqCst);
                            Ok(())
                        },
                        move |_| {
                            graceful_calls.fetch_add(1, Ordering::SeqCst);
                            if index == 1 {
                                trigger_timers.trigger(0);
                            }
                            Ok(())
                        },
                    ))
                },
            )
            .expect("register component");
    }

    let application = builder.build_all().expect("build components");
    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);
    let wake_counter = Arc::new(CountWake(AtomicUsize::new(0)));
    let waker = Waker::from(Arc::clone(&wake_counter));
    let mut context = Context::from_waker(&waker);
    let mut wait = Box::pin(shutdown.wait());

    let result = match wait.as_mut().poll(&mut context) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("ready overall deadline should win at batch boundary"),
    };
    let error = result.expect_err("overall deadline should be reported");
    assert!(error.report().overall_failure().is_some());
    assert_eq!(graceful_calls.load(Ordering::SeqCst), 64);
    // The first 64 entries synchronously confirm termination through their
    // stop callbacks; expiry aborts the one entry not visited yet.
    assert_eq!(abort_calls.load(Ordering::SeqCst), COUNT);
    assert_eq!(timers.durations(), [std::time::Duration::from_secs(30)]);
    // The timer wakes its registered task once; the driver must not also
    // issue the cooperative self-wake after the deadline is already ready.
    assert_eq!(wake_counter.0.load(Ordering::SeqCst), 1);
}

#[test]
fn test_newly_started_ready_wait_wins_before_overall_deadline_at_batch_boundary() {
    let timers = Timers::default();
    let trigger_timers = timers.clone();
    let graceful_calls = Arc::new(AtomicUsize::new(0));
    let ready_wait_polls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().wait_policy(timers.policy_with_total(std::time::Duration::from_secs(30)));
    for index in 0..65 {
        let trigger_timers = trigger_timers.clone();
        let graceful_calls = Arc::clone(&graceful_calls);
        let ready_wait_polls = Arc::clone(&ready_wait_polls);
        builder
            .register_managed_factory_with::<usize, _>(
                &[],
                BindingOptions {
                    id: Some(format!("resource.n{index}")),
                    ..BindingOptions::default()
                },
                move |_| {
                    if index == 1 {
                        Ok(Managed::asynchronous_with_graceful(
                            Arc::new(index),
                            |_| Ok(()),
                            move |_| {
                                graceful_calls.fetch_add(1, Ordering::SeqCst);
                                trigger_timers.trigger(0);
                                Ok(())
                            },
                            move |_| {
                                let ready_wait_polls = Arc::clone(&ready_wait_polls);
                                Box::pin(std::future::poll_fn(move |_| {
                                    ready_wait_polls.fetch_add(1, Ordering::SeqCst);
                                    Poll::Ready(Ok(()))
                                }))
                            },
                        ))
                    } else {
                        Ok(Managed::synchronous_with_graceful(
                            Arc::new(index),
                            |_| Ok(()),
                            move |_| {
                                graceful_calls.fetch_add(1, Ordering::SeqCst);
                                Ok(())
                            },
                        ))
                    }
                },
            )
            .expect("register component");
    }

    let application = builder.build_all().expect("build components");
    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);
    let mut wait = Box::pin(shutdown.wait());
    let result = support::shutdown_timer::ready(wait.as_mut());

    let error = result.expect_err("overall deadline should be reported");
    assert!(error.report().overall_failure().is_some());
    assert_eq!(graceful_calls.load(Ordering::SeqCst), 64);
    assert_eq!(ready_wait_polls.load(Ordering::SeqCst), 1);
}
