// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::future::Future;
use std::pin::Pin;
use std::pin::pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;
use std::time::Duration;

use qubit_ioc::WaitPolicy;

#[derive(Clone, Default)]
pub struct Gate {
    state: Arc<GateState>,
}

#[derive(Default)]
struct GateState {
    fired: AtomicBool,
    waker: Mutex<Option<Waker>>,
}

impl Gate {
    /// Completes this gate and wakes its observer after releasing the lock.
    pub fn trigger(&self) {
        self.state.fired.store(true, Ordering::SeqCst);
        let waker = self.state.waker.lock().expect("gate lock").take();
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl Future for Gate {
    type Output = ();

    /// Registers a waiter and checks again so a concurrent trigger is retained.
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        if self.state.fired.load(Ordering::SeqCst) {
            return Poll::Ready(());
        }
        *self.state.waker.lock().expect("gate lock") = Some(context.waker().clone());
        if self.state.fired.load(Ordering::SeqCst) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

#[derive(Clone, Default)]
pub struct Timers {
    calls: Arc<Mutex<Vec<TimerCall>>>,
}

struct TimerCall {
    duration: Duration,
    gate: Gate,
}

impl Timers {
    /// Supplies distinct grace and termination budgets with one gate per call.
    pub fn policy(&self) -> WaitPolicy {
        let timers = self.clone();
        WaitPolicy::bounded(Duration::from_secs(13), Duration::from_secs(7), move |duration| {
            let gate = Gate::default();
            timers.calls.lock().expect("timers lock").push(TimerCall {
                duration,
                gate: gate.clone(),
            });
            Box::pin(gate)
        })
    }

    /// Returns all requested budgets in creation order.
    pub fn durations(&self) -> Vec<Duration> {
        self.calls
            .lock()
            .expect("timers lock")
            .iter()
            .map(|call| call.duration)
            .collect()
    }

    /// Fires exactly one deadline, outside the timer registry lock.
    pub fn trigger(&self, index: usize) {
        let gate = self.calls.lock().expect("timers lock")[index].gate.clone();
        gate.trigger();
    }
}

// Dropping this borrowed future models cancellation; the handle must retain its
// work.
pub fn poll_once<F: Future>(future: F) -> Poll<F::Output> {
    pin!(future).as_mut().poll(&mut Context::from_waker(Waker::noop()))
}

/// Extracts a synchronously available outcome without introducing a runtime.
pub fn ready<F: Future>(future: F) -> F::Output {
    match poll_once(future) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("future should finish in this poll"),
    }
}
