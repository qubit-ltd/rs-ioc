// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

#[allow(dead_code)]
mod support;

use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use qubit_ioc::Application;
use qubit_ioc::BindingKey;
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

struct Worker;

#[derive(Default)]
struct Probe {
    aborts: AtomicUsize,
    gracefuls: AtomicUsize,
    wait_creations: AtomicUsize,
    ticket_drops: AtomicUsize,
    observed: Mutex<Vec<u64>>,
}

struct Ticket {
    id: u64,
    probe: Arc<Probe>,
}

impl Ticket {
    /// Creates a tracked request ticket with the supplied identity.
    fn new(id: u64, probe: &Arc<Probe>) -> Self {
        Self {
            id,
            probe: Arc::clone(probe),
        }
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        self.probe.ticket_drops.fetch_add(1, Ordering::SeqCst);
    }
}

/// Registers and builds one managed worker with an unbounded wait policy.
fn application(managed: Managed<Worker>) -> Application {
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_factory::<Worker, _>(&[], move |_| Ok(managed))
        .expect("register worker");
    builder.build_all().expect("build worker")
}

/// Supplies a request that returns ticket 7 and a wait that records its
/// identity.
fn abort_only(probe: &Arc<Probe>, gate: Gate) -> Managed<Worker> {
    let request_probe = Arc::clone(probe);
    let wait_probe = Arc::clone(probe);
    Managed::asynchronous_with_ticket(
        Arc::new(Worker),
        move |_| {
            request_probe.aborts.fetch_add(1, Ordering::SeqCst);
            Ok(Ticket::new(7, &request_probe))
        },
        move |_, ticket: Ticket| {
            wait_probe.wait_creations.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                ticket
                    .probe
                    .observed
                    .lock()
                    .expect("observed ticket lock")
                    .push(ticket.id);
                gate.await;
                drop(ticket);
                Ok(())
            })
        },
    )
}

/// Supplies graceful ticket 1 and abort ticket 2, with optional request
/// failures.
fn graceful(probe: &Arc<Probe>, gate: Gate, graceful_fails: bool, abort_fails: bool) -> Managed<Worker> {
    let abort_probe = Arc::clone(probe);
    let graceful_probe = Arc::clone(probe);
    let wait_probe = Arc::clone(probe);
    Managed::asynchronous_with_graceful_ticket(
        Arc::new(Worker),
        move |_| {
            abort_probe.aborts.fetch_add(1, Ordering::SeqCst);
            if abort_fails {
                Err(CleanupError::new(std::io::Error::other("abort request failed")))
            } else {
                Ok(Ticket::new(2, &abort_probe))
            }
        },
        move |_| {
            graceful_probe.gracefuls.fetch_add(1, Ordering::SeqCst);
            if graceful_fails {
                Err(CleanupError::new(std::io::Error::other("graceful request failed")))
            } else {
                Ok(Ticket::new(1, &graceful_probe))
            }
        },
        move |_, ticket: Ticket| {
            wait_probe.wait_creations.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                ticket
                    .probe
                    .observed
                    .lock()
                    .expect("observed ticket lock")
                    .push(ticket.id);
                gate.await;
                drop(ticket);
                Ok(())
            })
        },
    )
}

/// Reads all ticket identities observed by the wait future.
fn observed(probe: &Probe) -> Vec<u64> {
    probe.observed.lock().expect("observed ticket lock").clone()
}

/// Reads one counter without hiding which operation it represents.
fn count(counter: &AtomicUsize) -> usize {
    counter.load(Ordering::SeqCst)
}

#[test]
fn test_asynchronous_with_ticket_passes_abort_ticket_to_wait() {
    let probe = Arc::new(Probe::default());
    let gate = Gate::default();
    gate.trigger();
    let application = application(abort_only(&probe, gate));
    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);
    assert!(ready(shutdown.wait()).expect("abort wait completes").is_success());
    assert_eq!(observed(&probe), [7]);
    assert_eq!(count(&probe.aborts), 1);
    assert_eq!(count(&probe.wait_creations), 1);
    assert_eq!(count(&probe.ticket_drops), 1);
}

#[test]
fn test_graceful_ticket_passes_original_ticket_to_wait() {
    let probe = Arc::new(Probe::default());
    let gate = Gate::default();
    gate.trigger();
    let application = application(graceful(&probe, gate, false, false));
    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);
    assert!(ready(shutdown.wait()).expect("graceful wait completes").is_success());
    assert_eq!(observed(&probe), [1]);
    assert_eq!(count(&probe.gracefuls), 1);
    assert_eq!(count(&probe.aborts), 0);
    assert_eq!(count(&probe.wait_creations), 1);
    assert_eq!(count(&probe.ticket_drops), 1);
}

#[test]
fn test_abort_after_wait_started_preserves_graceful_ticket_and_future() {
    let probe = Arc::new(Probe::default());
    let gate = Gate::default();
    let application = application(graceful(&probe, gate.clone(), false, false));
    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);
    assert!(poll_once(shutdown.wait()).is_pending());
    assert_eq!(observed(&probe), [1]);
    shutdown.abort();
    assert_eq!(count(&probe.ticket_drops), 1, "unused abort ticket is dropped");
    gate.trigger();
    assert!(ready(shutdown.wait()).expect("upgraded wait completes").is_success());
    assert_eq!(observed(&probe), [1]);
    assert_eq!(count(&probe.gracefuls), 1);
    assert_eq!(count(&probe.aborts), 1);
    assert_eq!(count(&probe.wait_creations), 1);
    assert_eq!(count(&probe.ticket_drops), 2);
}

#[test]
fn test_request_scoped_ticket_cannot_confirm_termination_after_upgrade() {
    let graceful_request_done = Gate::default();
    let immediate_request_done = Gate::default();
    let timers = Timers::default();
    let graceful_wait = graceful_request_done.clone();
    let immediate_wait = immediate_request_done.clone();
    let managed = Managed::asynchronous_with_graceful_ticket(
        Arc::new(Worker),
        |_| Ok::<_, CleanupError>(immediate_wait),
        |_| Ok::<_, CleanupError>(graceful_wait),
        |_, ticket: Gate| {
            Box::pin(async move {
                ticket.await;
                Ok(())
            })
        },
    );
    let mut builder = ContainerBuilder::new().wait_policy(timers.policy());
    builder
        .register_managed_factory::<Worker, _>(&[], move |_| Ok(managed))
        .expect("register worker");
    let application = builder.build_all().expect("build worker");

    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);
    assert!(poll_once(shutdown.wait()).is_pending());
    shutdown.abort();
    immediate_request_done.trigger();
    assert!(
        poll_once(shutdown.wait()).is_pending(),
        "the old ticket still waits for its own request"
    );
    timers.trigger(0);
    assert!(poll_once(shutdown.wait()).is_pending());
    timers.trigger(1);
    let error = ready(shutdown.wait()).expect_err("request-scoped ticket cannot confirm final termination");
    assert_eq!(error.report().incomplete(), [BindingKey::of::<Worker>(None)]);
    assert_eq!(timers.durations(), [Duration::from_secs(13), Duration::from_secs(7)]);
}

#[test]
fn test_abort_before_first_wait_poll_uses_abort_ticket() {
    let probe = Arc::new(Probe::default());
    let gate = Gate::default();
    gate.trigger();
    let application = application(graceful(&probe, gate, false, false));
    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);
    shutdown.abort();
    assert!(ready(shutdown.wait()).expect("abort wait completes").is_success());
    assert_eq!(observed(&probe), [2]);
    assert_eq!(count(&probe.gracefuls), 0);
    assert_eq!(count(&probe.aborts), 1);
    assert_eq!(count(&probe.wait_creations), 1);
    assert_eq!(count(&probe.ticket_drops), 1);
}

#[test]
fn test_graceful_request_failure_falls_back_to_abort_ticket() {
    let probe = Arc::new(Probe::default());
    let gate = Gate::default();
    gate.trigger();
    let application = application(graceful(&probe, gate, true, false));
    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);
    let error = ready(shutdown.wait()).expect_err("graceful request failure is reported");
    assert_eq!(observed(&probe), [2]);
    assert_eq!(error.report().failures().len(), 1);
    assert_eq!(error.report().failures()[0].phase, ShutdownPhase::RequestGraceful);
    assert_eq!(count(&probe.gracefuls), 1);
    assert_eq!(count(&probe.aborts), 1);
    assert_eq!(count(&probe.wait_creations), 1);
    assert_eq!(count(&probe.ticket_drops), 1);
}

#[test]
fn test_abort_request_failure_without_ticket_reports_original_error() {
    let probe = Arc::new(Probe::default());
    let gate = Gate::default();
    let application = application(graceful(&probe, gate, false, true));
    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);
    let error = ready(shutdown.wait()).expect_err("abort failure is reported without panic");
    let abort_failure = error
        .report()
        .failures()
        .iter()
        .find(|failure| failure.phase == ShutdownPhase::Abort)
        .expect("abort request failure retained");
    assert_eq!(
        abort_failure
            .error
            .source()
            .expect("original request error")
            .to_string(),
        "abort request failed"
    );
    assert!(
        error
            .report()
            .failures()
            .iter()
            .any(|failure| failure.phase == ShutdownPhase::Wait)
    );
    assert_eq!(count(&probe.aborts), 1);
    assert_eq!(count(&probe.gracefuls), 0);
    assert_eq!(count(&probe.wait_creations), 0);
    assert_eq!(count(&probe.ticket_drops), 0);
}

#[test]
fn test_cancel_and_resume_wait_reuses_ticket_and_future() {
    let probe = Arc::new(Probe::default());
    let gate = Gate::default();
    let application = application(graceful(&probe, gate.clone(), false, false));
    let mut shutdown = application.begin_shutdown(ShutdownMode::Graceful);
    assert!(poll_once(shutdown.wait()).is_pending());
    assert!(poll_once(shutdown.wait()).is_pending());
    assert_eq!(observed(&probe), [1]);
    assert_eq!(count(&probe.gracefuls), 1);
    assert_eq!(count(&probe.aborts), 0);
    assert_eq!(count(&probe.wait_creations), 1);
    assert_eq!(count(&probe.ticket_drops), 0);
    gate.trigger();
    assert!(ready(shutdown.wait()).expect("resumed wait completes").is_success());
    assert_eq!(count(&probe.ticket_drops), 1);
}

#[test]
fn test_drop_untransferred_managed_only_requests_abort() {
    let probe = Arc::new(Probe::default());
    drop(graceful(&probe, Gate::default(), false, false));
    assert_eq!(count(&probe.aborts), 1);
    assert_eq!(count(&probe.gracefuls), 0);
    assert_eq!(count(&probe.wait_creations), 0);
    assert_eq!(count(&probe.ticket_drops), 1);
}
