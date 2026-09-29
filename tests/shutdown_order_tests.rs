// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use qubit_ioc::Application;
use qubit_ioc::ApplicationState;
use qubit_ioc::BindingOptions;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Definition;
use qubit_ioc::Dependency;
use qubit_ioc::Managed;
use qubit_ioc::ShutdownMode;
use qubit_ioc::WaitPolicy;

struct A(AtomicBool);
struct B(AtomicBool);
struct C;
trait Service: Send + Sync {}
impl Service for C {}

type Log = Arc<Mutex<Vec<String>>>;

/// Builds a real dependency chain whose consumer also exposes a trait alias.
fn chain(check_dependencies: bool) -> (Application, Log, Arc<AtomicUsize>) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let factories = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    let events = Arc::clone(&log);
    builder
        .register_managed_factory::<A, _>(&[], move |_| {
            let abort = Arc::clone(&events);
            let grace = Arc::clone(&events);
            Ok(Managed::new(Arc::new(A(AtomicBool::new(true))), move |value| {
                value.0.store(false, Ordering::SeqCst);
                abort.lock().expect("log lock").push("abort A".into());
                Ok(())
            })
            .with_graceful_stop(move |value| {
                value.0.store(false, Ordering::SeqCst);
                grace.lock().expect("log lock").push("grace A".into());
                Ok(())
            })
            .with_wait(move |_| {
                Box::pin(async move {
                    events.lock().expect("log lock").push("wait A".into());
                    Ok(())
                })
            }))
        })
        .expect("register A");
    let events = Arc::clone(&log);
    builder
        .register_managed_factory::<B, _>(&[Dependency::of::<A>()], move |context| {
            context.get::<A>().expect("A dependency");
            let abort = Arc::clone(&events);
            let grace = Arc::clone(&events);
            Ok(Managed::new(Arc::new(B(AtomicBool::new(true))), move |value| {
                value.0.store(false, Ordering::SeqCst);
                abort.lock().expect("log lock").push("abort B".into());
                Ok(())
            })
            .with_graceful_stop(move |value| {
                value.0.store(false, Ordering::SeqCst);
                grace.lock().expect("log lock").push("grace B".into());
                Ok(())
            })
            .with_wait(move |_| {
                Box::pin(async move {
                    events.lock().expect("log lock").push("wait B".into());
                    Ok(())
                })
            }))
        })
        .expect("register B");
    let events = Arc::clone(&log);
    let calls = Arc::clone(&factories);
    let definition = Definition::<C>::builder()
        .dependencies(&[Dependency::of::<A>(), Dependency::of::<B>()])
        .managed_factory(move |context| {
            calls.fetch_add(1, Ordering::SeqCst);
            let a = context.get::<A>().expect("A dependency");
            let b = context.get::<B>().expect("B dependency");
            let abort = Arc::clone(&events);
            let grace = Arc::clone(&events);
            Ok(Managed::new(Arc::new(C), move |_| {
                abort.lock().expect("log lock").push("abort C".into());
                Ok(())
            })
            .with_graceful_stop(move |_| {
                grace.lock().expect("log lock").push("grace C".into());
                Ok(())
            })
            .with_wait(move |_| {
                Box::pin(async move {
                    if check_dependencies {
                        assert!(a.0.load(Ordering::SeqCst), "A must run until C completes");
                        assert!(b.0.load(Ordering::SeqCst), "B must run until C completes");
                    }
                    events.lock().expect("log lock").push("wait C".into());
                    Ok(())
                })
            }))
        })
        .bind::<dyn Service, _>(BindingOptions::default(), |value| value)
        .build()
        .expect("complete C definition");
    builder.register_definition(definition).expect("register C");
    (builder.build_all().expect("build chain"), log, factories)
}

#[test]
fn test_graceful_wait_preserves_running_dependencies_and_alias_identity() {
    let (application, log, calls) = chain(true);
    let concrete = application.context().get::<C>().expect("C");
    let alias = application.context().get::<dyn Service>().expect("service alias");
    let projected: Arc<dyn Service> = concrete;
    assert!(Arc::ptr_eq(&projected, &alias));
    let mut handle = application.begin_shutdown(ShutdownMode::Graceful);
    let mut wait = pin!(handle.wait());
    let Poll::Ready(result) = wait.as_mut().poll(&mut Context::from_waker(Waker::noop())) else {
        panic!("ready waits finish");
    };
    assert!(result.expect("graceful success").is_success());
    assert_eq!(
        *log.lock().expect("log lock"),
        ["grace C", "wait C", "grace B", "wait B", "grace A", "wait A"]
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn test_immediate_aborts_all_before_return_then_waits_in_reverse_order() {
    let (application, log, _) = chain(false);
    let mut handle = application.begin_shutdown(ShutdownMode::Immediate);
    assert_eq!(*log.lock().expect("log lock"), ["abort C", "abort B", "abort A"]);
    let mut wait = pin!(handle.wait());
    let Poll::Ready(result) = wait.as_mut().poll(&mut Context::from_waker(Waker::noop())) else {
        panic!("ready waits finish");
    };
    assert!(result.expect("immediate success").is_success());
    assert_eq!(
        *log.lock().expect("log lock"),
        ["abort C", "abort B", "abort A", "wait C", "wait B", "wait A"]
    );
}

#[test]
fn test_graceful_handle_creation_is_lazy_and_unpolled_drop_only_aborts() {
    let (application, log, _) = chain(false);
    let shared = application.context().clone();
    let handle = application.begin_shutdown(ShutdownMode::Graceful);
    assert_eq!(shared.state(), ApplicationState::ShuttingDown);
    assert!(log.lock().expect("log lock").is_empty());
    drop(handle);
    assert_eq!(*log.lock().expect("log lock"), ["abort C", "abort B", "abort A"]);
    assert_eq!(shared.state(), ApplicationState::Incomplete);
}
