// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Cooperative scheduling of immediately completed asynchronous builds.

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
use qubit_ioc::BuildSessionError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Definition;
use qubit_ioc::FactoryError;
use qubit_ioc::Managed;
use qubit_ioc::WaitPolicy;

struct Concrete;

trait Projected: Send + Sync {
    fn value(&self) -> usize;
}

impl Projected for Concrete {
    fn value(&self) -> usize {
        17
    }
}

struct CountWake(AtomicUsize);

impl Wake for CountWake {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn ready<F: Future>(future: F) -> F::Output {
    match Box::pin(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("expected ready future"),
    }
}

fn immediate_builder(count: usize, calls: Arc<AtomicUsize>) -> ContainerBuilder {
    let mut builder = ContainerBuilder::new();
    for index in 0..count {
        let calls = Arc::clone(&calls);
        builder
            .register_async_factory_with::<usize, _>(
                &[],
                BindingOptions {
                    id: Some(format!("service.n{index}")),
                    ..BindingOptions::default()
                },
                move |_| {
                    Box::pin(async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        Ok(Arc::new(index))
                    })
                },
            )
            .expect("register factory");
    }
    builder
}

#[test]
fn test_immediately_ready_build_yields_after_one_batch_and_preserves_order() {
    const BINDING_COUNT: usize = 257;

    let executed = Arc::new(Mutex::new(Vec::new()));
    let mut builder = ContainerBuilder::new();
    for index in 0..BINDING_COUNT {
        let executed = Arc::clone(&executed);
        builder
            .register_async_factory_with::<usize, _>(
                &[],
                BindingOptions {
                    id: Some(format!("service.n{index}")),
                    ..BindingOptions::default()
                },
                move |_| {
                    Box::pin(async move {
                        executed.lock().expect("execution log").push(index);
                        Ok(Arc::new(index))
                    })
                },
            )
            .expect("register factory");
    }

    let wake_counter = Arc::new(CountWake(AtomicUsize::new(0)));
    let waker = Waker::from(Arc::clone(&wake_counter));
    let mut context = Context::from_waker(&waker);
    let mut build = Box::pin(builder.build_all_async());

    assert!(build.as_mut().poll(&mut context).is_pending());
    assert!(executed.lock().expect("execution log").len() < BINDING_COUNT);
    assert!(wake_counter.0.load(Ordering::SeqCst) > 0);

    let mut result = None;
    for _ in 0..8 {
        match build.as_mut().poll(&mut context) {
            Poll::Ready(value) => {
                result = Some(value);
                break;
            }
            Poll::Pending => {}
        }
    }
    let application = result
        .expect("bounded polling completes the build")
        .expect("build succeeds");
    assert_eq!(
        *executed.lock().expect("execution log"),
        (0..BINDING_COUNT).collect::<Vec<_>>()
    );
    drop(application);
}

#[test]
fn test_empty_and_batch_boundary_builds_only_yield_when_more_work_remains() {
    for count in [0, 1, 63, 64, 65] {
        let calls = Arc::new(AtomicUsize::new(0));
        let builder = immediate_builder(count, Arc::clone(&calls));
        let wake_counter = Arc::new(CountWake(AtomicUsize::new(0)));
        let waker = Waker::from(Arc::clone(&wake_counter));
        let mut context = Context::from_waker(&waker);
        let mut build = Box::pin(builder.build_all_async());
        let poll = build.as_mut().poll(&mut context);

        if count <= 64 {
            assert!(
                poll.is_ready(),
                "{count} bindings should complete in one poll"
            );
            assert_eq!(wake_counter.0.load(Ordering::SeqCst), 0);
        } else {
            assert!(
                poll.is_pending(),
                "65 bindings should yield after the first batch"
            );
            assert_eq!(wake_counter.0.load(Ordering::SeqCst), 1);
            assert!(build.as_mut().poll(&mut context).is_ready());
        }
        assert_eq!(calls.load(Ordering::SeqCst), count);
    }
}

#[test]
fn test_factory_failure_after_a_yield_stops_the_remaining_order() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut builder = immediate_builder(64, Arc::clone(&calls));
    builder
        .register_async_factory_with::<usize, _>(
            &[],
            BindingOptions {
                id: Some("service.failure".to_owned()),
                ..BindingOptions::default()
            },
            |_| {
                Box::pin(async {
                    Err(FactoryError::new(std::io::Error::other("expected failure")))
                })
            },
        )
        .expect("register failing factory");
    let mut build = Box::pin(builder.build_all_async());
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);

    assert!(build.as_mut().poll(&mut context).is_pending());
    let failure = match build.as_mut().poll(&mut context) {
        Poll::Ready(Err(failure)) => failure,
        _ => panic!("expected factory failure"),
    };
    assert_eq!(calls.load(Ordering::SeqCst), 64);
    assert!(failure.to_string().contains("expected failure"));
}

#[test]
fn test_sync_async_and_alias_bindings_construct_once_and_remain_queryable() {
    let concrete_calls = Arc::new(AtomicUsize::new(0));
    let alias_calls = Arc::new(AtomicUsize::new(0));
    let sync_calls = Arc::new(AtomicUsize::new(0));
    let async_calls = Arc::new(AtomicUsize::new(0));
    let concrete_count = Arc::clone(&concrete_calls);
    let alias_count = Arc::clone(&alias_calls);
    let definition = Definition::<Concrete>::builder()
        .factory(move |_| {
            concrete_count.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(Concrete))
        })
        .bind::<dyn Projected, _>(BindingOptions::default(), move |value| {
            alias_count.fetch_add(1, Ordering::SeqCst);
            value
        })
        .build()
        .expect("definition");

    let mut builder = ContainerBuilder::new();
    builder
        .register_definition(definition)
        .expect("register alias definition");
    let sync_count = Arc::clone(&sync_calls);
    builder
        .register_factory::<String, _>(&[], move |_| {
            sync_count.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new("sync".to_owned()))
        })
        .expect("register sync factory");
    let async_count = Arc::clone(&async_calls);
    builder
        .register_async_factory::<u8, _>(&[], move |_| {
            Box::pin(async move {
                async_count.fetch_add(1, Ordering::SeqCst);
                Ok(Arc::new(8))
            })
        })
        .expect("register async factory");

    let application = ready(builder.build_all_async()).expect("build mixed graph");
    let context = application.context();
    assert_eq!(
        context.get::<String>().expect("sync value").as_str(),
        "sync"
    );
    assert_eq!(*context.get::<u8>().expect("async value"), 8);
    assert_eq!(context.get::<dyn Projected>().expect("alias").value(), 17);
    assert_eq!(concrete_calls.load(Ordering::SeqCst), 1);
    assert_eq!(alias_calls.load(Ordering::SeqCst), 1);
    assert_eq!(sync_calls.load(Ordering::SeqCst), 1);
    assert_eq!(async_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn test_cancelled_build_session_aborts_completed_batch_once_and_cleanup_is_observable() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    for index in 0..65 {
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
                        aborts.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    }))
                },
            )
            .expect("register managed factory");
    }
    let mut session = builder.build_all_async_session();
    let wake_counter = Arc::new(CountWake(AtomicUsize::new(0)));
    let waker = Waker::from(Arc::clone(&wake_counter));
    let mut context = Context::from_waker(&waker);
    let mut run = Box::pin(session.run());

    assert!(run.as_mut().poll(&mut context).is_pending());
    assert_eq!(aborts.load(Ordering::SeqCst), 0);
    assert!(wake_counter.0.load(Ordering::SeqCst) > 0);
    drop(run);
    assert_eq!(aborts.load(Ordering::SeqCst), 64);
    assert!(matches!(
        ready(session.run()),
        Err(BuildSessionError::Cancelled)
    ));
    assert!(
        ready(session.wait_cancelled_cleanup())
            .expect("cleanup report")
            .is_success()
    );
    assert!(
        ready(session.wait_cancelled_cleanup())
            .expect("same cleanup report")
            .is_success()
    );
    assert_eq!(aborts.load(Ordering::SeqCst), 64);
}

#[test]
fn test_cancelled_ordinary_async_build_aborts_completed_batch() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    for index in 0..65 {
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
                        aborts.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    }))
                },
            )
            .expect("register managed factory");
    }
    let mut build = Box::pin(builder.build_all_async());
    let mut context = Context::from_waker(Waker::noop());
    assert!(build.as_mut().poll(&mut context).is_pending());
    assert_eq!(aborts.load(Ordering::SeqCst), 0);
    drop(build);
    assert_eq!(aborts.load(Ordering::SeqCst), 64);
}
