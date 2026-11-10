// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Cooperative scheduling of immediately completed shutdown observations.

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

struct CountWake(AtomicUsize);

impl Wake for CountWake {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn immediate_shutdown_builder(
    count: usize,
    observed: Arc<Mutex<Vec<(char, usize)>>>,
) -> ContainerBuilder {
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

fn synchronous_shutdown_builder(
    count: usize,
    observed: Arc<Mutex<Vec<usize>>>,
) -> ContainerBuilder {
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
    assert_eq!(
        *observed.lock().expect("log"),
        (0..COUNT).rev().collect::<Vec<_>>()
    );
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
            assert!(
                poll.is_pending(),
                "65 entries should yield after the first batch"
            );
            assert_eq!(wake_counter.0.load(Ordering::SeqCst), 1);
            assert!(wait.as_mut().poll(&mut context).is_ready());
        }
        assert_eq!(observed.lock().expect("log").len(), count);
    }
}
