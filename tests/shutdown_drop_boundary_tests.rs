// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

mod support;

use std::any::Any;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::time::Duration;

use qubit_ioc::CleanupError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Managed;
use qubit_ioc::ShutdownMode;
use support::shutdown_timer::Timers;
use support::shutdown_timer::poll_once;
use support::shutdown_timer::ready;

const DROP_PANIC: &str = "shutdown wait future drop panicked";

/// A wait future whose poll result and one-time destructor panic are controlled
/// by the test.
struct DropPanicFuture {
    ready: bool,
    panicked: Arc<AtomicBool>,
}

impl Future for DropPanicFuture {
    type Output = Result<(), CleanupError>;

    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        if self.ready { Poll::Ready(Ok(())) } else { Poll::Pending }
    }
}

impl Drop for DropPanicFuture {
    fn drop(&mut self) {
        if !self.panicked.swap(true, Ordering::SeqCst) {
            panic!("{DROP_PANIC}");
        }
    }
}

fn application(
    ready: bool,
    timers: &Timers,
    aborts: Arc<AtomicUsize>,
    drops: Arc<AtomicBool>,
) -> qubit_ioc::Application {
    let policy = timers.policy();
    let mut builder = ContainerBuilder::new().wait_policy(policy);
    builder
        .register_managed_factory::<u32, _>(&[], move |_| {
            let future_drops = Arc::clone(&drops);
            Ok(Managed::asynchronous(
                Arc::new(1),
                move |_| {
                    aborts.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
                move |_| {
                    Box::pin(DropPanicFuture {
                        ready,
                        panicked: future_drops,
                    })
                },
            ))
        })
        .expect("register managed component");
    builder.build_all().expect("build component")
}

fn panic_text(payload: &(dyn Any + Send)) -> Option<&str> {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
}

#[test]
fn ready_completion_future_drop_panic_propagates() {
    let timers = Timers::default();
    let aborts = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(AtomicBool::new(false));
    let mut shutdown =
        application(true, &timers, Arc::clone(&aborts), Arc::clone(&dropped)).begin_shutdown(ShutdownMode::Immediate);

    let result = catch_unwind(AssertUnwindSafe(|| ready(shutdown.wait())));
    let payload = result.expect_err("future destructor panic propagates");
    assert_eq!(panic_text(payload.as_ref()), Some(DROP_PANIC));
    assert!(dropped.load(Ordering::SeqCst));
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
    let report = shutdown.abandon();
    assert!(
        report.is_success(),
        "ready completion was finalized before destructor panic"
    );
}

#[test]
fn termination_deadline_future_drop_panic_propagates_without_final_report() {
    let timers = Timers::default();
    let aborts = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(AtomicBool::new(false));
    let mut shutdown =
        application(false, &timers, Arc::clone(&aborts), Arc::clone(&dropped)).begin_shutdown(ShutdownMode::Immediate);

    assert!(poll_once(shutdown.wait()).is_pending());
    assert_eq!(timers.durations(), [Duration::from_secs(7)]);
    timers.trigger(0);
    let result = catch_unwind(AssertUnwindSafe(|| poll_once(shutdown.wait())));
    let payload = result.expect_err("dropping timed-out future propagates panic");
    assert_eq!(panic_text(payload.as_ref()), Some(DROP_PANIC));
    assert!(dropped.load(Ordering::SeqCst));
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
    assert_eq!(
        shutdown.pending().len(),
        1,
        "deadline path retains the pending component"
    );
    let report = shutdown.abandon();
    assert_eq!(report.incomplete().len(), 1);
    assert_eq!(aborts.load(Ordering::SeqCst), 1, "abandon does not repeat abort");
}

#[test]
fn abandon_after_cancelled_wait_future_drop_panic_propagates() {
    let timers = Timers::default();
    let aborts = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(AtomicBool::new(false));
    let mut shutdown =
        application(false, &timers, Arc::clone(&aborts), Arc::clone(&dropped)).begin_shutdown(ShutdownMode::Immediate);

    assert!(poll_once(shutdown.wait()).is_pending());
    let result = catch_unwind(AssertUnwindSafe(|| shutdown.abandon()));
    let payload = result.expect_err("abandon drops retained future and propagates panic");
    assert_eq!(panic_text(payload.as_ref()), Some(DROP_PANIC));
    assert!(dropped.load(Ordering::SeqCst));
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
}
