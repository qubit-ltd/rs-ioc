// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use qubit_ioc::__private::codegen_v1::DefinitionDraft;
use qubit_ioc::BindingOptions;
use qubit_ioc::BuildError;
use qubit_ioc::CleanupError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::DefinitionSource;
use qubit_ioc::Dependency;
use qubit_ioc::FactoryError;
use qubit_ioc::Managed;
use qubit_ioc::ShutdownPhase;

struct First;
struct Second;
struct Third;

fn run_ready<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("future unexpectedly yielded"),
    }
}

fn record_managed<T: Send + Sync + 'static>(
    value: T,
    name: &'static str,
    events: Arc<Mutex<Vec<&'static str>>>,
) -> Managed<T> {
    let stop_events = Arc::clone(&events);
    let wait_events = Arc::clone(&events);
    Managed::new(Arc::new(value), move |_| {
        stop_events.lock().unwrap().push(name);
        Ok(())
    })
    .with_wait(move |_| {
        Box::pin(async move {
            wait_events.lock().unwrap().push(match name {
                "stop A" => "wait A",
                "stop B" => "wait B",
                "stop C" => "wait C",
                other => other,
            });
            Ok(())
        })
    })
}

#[test]
fn test_shutdown_stops_then_waits_in_reverse_construction_order() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut builder = ContainerBuilder::new();
    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<First, _>(&[], move |_| Ok(record_managed(First, "stop A", captured)))
        .unwrap();
    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<Second, _>(&[Dependency::of::<First>()], move |context| {
            let _first = context.get::<First>().unwrap();
            Ok(record_managed(Second, "stop B", captured))
        })
        .unwrap();
    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<Third, _>(&[Dependency::of::<Second>()], move |context| {
            let _second = context.get::<Second>().unwrap();
            Ok(record_managed(Third, "stop C", captured))
        })
        .unwrap();

    let context = builder.build_all().unwrap();
    assert!(events.lock().unwrap().is_empty());
    let mut shutdown = context.begin_shutdown();
    run_ready(shutdown.wait()).unwrap();
    assert_eq!(
        *events.lock().unwrap(),
        ["stop C", "stop B", "stop A", "wait C", "wait B", "wait A"]
    );
}

#[test]
fn test_sync_build_failure_stops_completed_managed_factories_once() {
    let stops = Arc::new(AtomicUsize::new(0));
    let later_calls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();
    let captured = Arc::clone(&stops);
    builder
        .register_managed_factory::<First, _>(&[], move |_| {
            Ok(Managed::new(Arc::new(First), move |_| {
                captured.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }))
        })
        .unwrap();
    builder
        .register_factory::<Second, _>(&[], |_| Err(FactoryError::new(std::io::Error::other("failure"))))
        .unwrap();
    let later = Arc::clone(&later_calls);
    builder
        .register_factory::<Third, _>(&[], move |_| {
            later.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(Third))
        })
        .unwrap();

    assert!(matches!(builder.build_all(), Err(BuildError::FactoryFailed { .. })));
    assert_eq!(stops.load(Ordering::SeqCst), 1);
    assert_eq!(later_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn test_async_build_failure_waits_after_stopping_and_reports_both_phases() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut builder = ContainerBuilder::new();
    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<First, _>(&[], move |_| {
            let stop_events = Arc::clone(&captured);
            let wait_events = Arc::clone(&captured);
            Ok(Managed::new(Arc::new(First), move |_| {
                stop_events.lock().unwrap().push("stop");
                Err(CleanupError::new(std::io::Error::other("stop failure")))
            })
            .with_wait(move |_| {
                Box::pin(async move {
                    wait_events.lock().unwrap().push("wait");
                    Err(CleanupError::new(std::io::Error::other("wait failure")))
                })
            }))
        })
        .unwrap();
    builder
        .register_async_factory::<Second, _>(&[], |_| {
            Box::pin(async { Err(FactoryError::new(std::io::Error::other("factory failure"))) })
        })
        .unwrap();

    let error = match run_ready(builder.build_all_async()) {
        Ok(_) => panic!("the factory failure should abort the build"),
        Err(error) => error,
    };
    let BuildError::CleanupFailed { cause, failures } = error else {
        panic!("cleanup failures should be retained with the build error");
    };
    assert!(matches!(*cause, BuildError::FactoryFailed { .. }));
    assert_eq!(failures.len(), 2);
    assert_eq!(failures[0].phase, ShutdownPhase::Stop);
    assert_eq!(failures[1].phase, ShutdownPhase::Wait);
    assert_eq!(*events.lock().unwrap(), ["stop", "wait"]);
}

#[test]
fn test_shutdown_continues_after_multiple_stop_and_wait_failures() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut builder = ContainerBuilder::new();
    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<First, _>(&[], move |_| {
            let stop_events = Arc::clone(&captured);
            let wait_events = Arc::clone(&captured);
            Ok(Managed::new(Arc::new(First), move |_| {
                stop_events.lock().unwrap().push("stop A");
                Err(CleanupError::new(std::io::Error::other("stop failure")))
            })
            .with_wait(move |_| {
                Box::pin(async move {
                    wait_events.lock().unwrap().push("wait A");
                    Err(CleanupError::new(std::io::Error::other("wait failure")))
                })
            }))
        })
        .unwrap();
    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<Second, _>(&[Dependency::of::<First>()], move |context| {
            let _first = context.get::<First>().unwrap();
            let stop_events = Arc::clone(&captured);
            let wait_events = Arc::clone(&captured);
            Ok(Managed::new(Arc::new(Second), move |_| {
                stop_events.lock().unwrap().push("stop B");
                Err(CleanupError::new(std::io::Error::other("stop failure")))
            })
            .with_wait(move |_| {
                Box::pin(async move {
                    wait_events.lock().unwrap().push("wait B");
                    Err(CleanupError::new(std::io::Error::other("wait failure")))
                })
            }))
        })
        .unwrap();

    let context = builder.build_all().unwrap();
    let mut shutdown = context.begin_shutdown();
    let error = run_ready(shutdown.wait()).expect_err("cleanup failures are returned");
    assert_eq!(error.failures().len(), 4);
    let source = std::error::Error::source(&error.failures()[0]).expect("cleanup error source");
    assert!(std::error::Error::source(source).is_some());
    let repeated = run_ready(shutdown.wait()).expect_err("completed errors are repeatable");
    assert_eq!(repeated.failures().len(), 4);
    assert!(std::ptr::eq(error.failures().as_ptr(), repeated.failures().as_ptr()));
    assert_eq!(*events.lock().unwrap(), ["stop B", "stop A", "wait B", "wait A"]);
}

#[test]
fn test_shutdown_continues_after_stop_panic() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut builder = ContainerBuilder::new();

    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<First, _>(&[], move |_| {
            let stop_events = Arc::clone(&captured);
            let wait_events = Arc::clone(&captured);
            Ok(Managed::new(Arc::new(First), move |_| {
                stop_events.lock().unwrap().push("stop A");
                Ok(())
            })
            .with_wait(move |_| {
                Box::pin(async move {
                    wait_events.lock().unwrap().push("wait A");
                    Ok(())
                })
            }))
        })
        .unwrap();

    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<Second, _>(&[Dependency::of::<First>()], move |context| {
            let _first = context.get::<First>().unwrap();
            let stop_events = Arc::clone(&captured);
            let wait_events = Arc::clone(&captured);
            Ok(Managed::new(Arc::new(Second), move |_| {
                stop_events.lock().unwrap().push("stop B");
                panic!("stop B panicked");
            })
            .with_wait(move |_| {
                Box::pin(async move {
                    wait_events.lock().unwrap().push("wait B");
                    Ok(())
                })
            }))
        })
        .unwrap();

    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<Third, _>(&[Dependency::of::<Second>()], move |context| {
            let _second = context.get::<Second>().unwrap();
            let stop_events = Arc::clone(&captured);
            let wait_events = Arc::clone(&captured);
            Ok(Managed::new(Arc::new(Third), move |_| {
                stop_events.lock().unwrap().push("stop C");
                Ok(())
            })
            .with_wait(move |_| {
                Box::pin(async move {
                    wait_events.lock().unwrap().push("wait C");
                    Ok(())
                })
            }))
        })
        .unwrap();

    let context = builder.build_all().unwrap();
    let mut shutdown = context.begin_shutdown();
    let error = run_ready(shutdown.wait()).expect_err("stop panic is reported");

    assert_eq!(error.failures().len(), 1);
    assert_eq!(error.failures()[0].phase, ShutdownPhase::Stop);
    assert_eq!(error.failures()[0].key.type_name(), std::any::type_name::<Second>());
    let message = error.failures()[0].to_string();
    assert!(message.contains("stop B panicked"));
    assert!(message.contains("managed_lifecycle_tests.rs"));
    assert_eq!(
        *events.lock().unwrap(),
        ["stop C", "stop B", "stop A", "wait C", "wait B", "wait A"]
    );
}

#[test]
fn test_async_build_failure_continues_after_stop_panic() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut builder = ContainerBuilder::new();

    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<First, _>(&[], move |_| {
            let stop_events = Arc::clone(&captured);
            let wait_events = Arc::clone(&captured);
            Ok(Managed::new(Arc::new(First), move |_| {
                stop_events.lock().unwrap().push("stop A");
                Ok(())
            })
            .with_wait(move |_| {
                Box::pin(async move {
                    wait_events.lock().unwrap().push("wait A");
                    Ok(())
                })
            }))
        })
        .unwrap();

    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<Second, _>(&[], move |_| {
            let stop_events = Arc::clone(&captured);
            let wait_events = Arc::clone(&captured);
            Ok(Managed::new(Arc::new(Second), move |_| {
                stop_events.lock().unwrap().push("stop B");
                panic!("stop B panicked during build cleanup");
            })
            .with_wait(move |_| {
                Box::pin(async move {
                    wait_events.lock().unwrap().push("wait B");
                    Ok(())
                })
            }))
        })
        .unwrap();

    builder
        .register_factory::<Third, _>(&[Dependency::of::<First>(), Dependency::of::<Second>()], |_| {
            Err(FactoryError::new(std::io::Error::other("factory failure")))
        })
        .unwrap();

    let error = match run_ready(builder.build_all_async()) {
        Ok(_) => panic!("the factory failure should abort the build"),
        Err(error) => error,
    };
    let BuildError::CleanupFailed { cause, failures } = error else {
        panic!("stop panic must be retained alongside the factory failure");
    };
    assert!(matches!(*cause, BuildError::FactoryFailed { .. }));
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].phase, ShutdownPhase::Stop);
    assert!(failures[0].to_string().contains("stop B panicked during build cleanup"));
    assert_eq!(*events.lock().unwrap(), ["stop B", "stop A", "wait B", "wait A"]);
}

#[test]
fn test_cancelled_shutdown_wait_resumes_the_same_future() {
    let callback_calls = Arc::new(AtomicUsize::new(0));
    let future_polls = Arc::new(AtomicUsize::new(0));
    let stops = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();
    let captured_calls = Arc::clone(&callback_calls);
    let captured_polls = Arc::clone(&future_polls);
    let captured_stops = Arc::clone(&stops);
    builder
        .register_managed_factory::<First, _>(&[], move |_| {
            Ok(Managed::new(Arc::new(First), move |_| {
                captured_stops.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .with_wait(move |_| {
                captured_calls.fetch_add(1, Ordering::SeqCst);
                Box::pin(std::future::poll_fn(move |_| {
                    if captured_polls.fetch_add(1, Ordering::SeqCst) == 0 {
                        Poll::Pending
                    } else {
                        Poll::Ready(Ok(()))
                    }
                }))
            }))
        })
        .expect("register managed component");

    let context = builder.build_all().expect("build managed component");
    let mut shutdown = context.begin_shutdown();
    assert_eq!(stops.load(Ordering::SeqCst), 1);

    let mut first_wait = Box::pin(shutdown.wait());
    assert!(
        first_wait
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    drop(first_wait);

    let mut resumed_wait = Box::pin(shutdown.wait());
    assert!(matches!(
        resumed_wait.as_mut().poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Ok(()))
    ));
    drop(resumed_wait);
    assert!(run_ready(shutdown.wait()).is_ok());
    assert_eq!(stops.load(Ordering::SeqCst), 1);
    assert_eq!(callback_calls.load(Ordering::SeqCst), 1);
    assert_eq!(future_polls.load(Ordering::SeqCst), 2);
}

#[test]
fn test_cancelled_async_build_stops_completed_components_without_waiting() {
    let stops = Arc::new(AtomicUsize::new(0));
    let waits = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();
    let captured_stops = Arc::clone(&stops);
    let captured_waits = Arc::clone(&waits);
    builder
        .register_managed_factory::<First, _>(&[], move |_| {
            Ok(Managed::new(Arc::new(First), move |_| {
                captured_stops.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .with_wait(move |_| {
                Box::pin(async move {
                    captured_waits.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            }))
        })
        .unwrap();
    builder
        .register_async_factory::<Second, _>(&[], |_| Box::pin(std::future::pending()))
        .unwrap();

    let mut future = Box::pin(builder.build_all_async());
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    drop(future);
    assert_eq!(stops.load(Ordering::SeqCst), 1);
    assert_eq!(waits.load(Ordering::SeqCst), 0);
}

#[test]
fn test_cancelled_wait_phase_has_already_stopped_all_components() {
    let stops = Arc::new(AtomicUsize::new(0));
    let waits = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();
    let captured_stops = Arc::clone(&stops);
    let captured_waits = Arc::clone(&waits);
    builder
        .register_managed_factory::<First, _>(&[], move |_| {
            Ok(Managed::new(Arc::new(First), move |_| {
                captured_stops.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .with_wait(move |_| {
                Box::pin(async move {
                    captured_waits.fetch_add(1, Ordering::SeqCst);
                    std::future::pending::<Result<(), CleanupError>>().await
                })
            }))
        })
        .unwrap();
    builder
        .register_async_factory::<Second, _>(&[], |_| {
            Box::pin(async { Err(FactoryError::new(std::io::Error::other("factory failure"))) })
        })
        .unwrap();

    let mut future = Box::pin(builder.build_all_async());
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    drop(future);
    assert_eq!(stops.load(Ordering::SeqCst), 1);
    assert_eq!(waits.load(Ordering::SeqCst), 1);
}

trait Service: Send + Sync {}

struct ConcreteService;

impl Service for ConcreteService {}

#[test]
fn test_managed_concrete_definition_with_alias_stops_once() {
    let stops = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();
    let captured = Arc::clone(&stops);
    let mut draft = DefinitionDraft::<ConcreteService>::new_managed_sync(
        DefinitionSource::new(
            "tests",
            "managed_lifecycle_tests",
            "managed_lifecycle_tests.rs",
            1,
            1,
            "ConcreteService",
        ),
        &[],
        Default::default(),
        move |_| {
            Ok(Managed::new(Arc::new(ConcreteService), move |_| {
                captured.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }))
        },
    )
    .unwrap();
    draft
        .bind::<dyn Service, _>(Default::default(), |concrete| -> Arc<dyn Service> { concrete })
        .unwrap();
    draft.register(&mut builder).unwrap();
    builder.root::<dyn Service>();

    let context = builder.build().unwrap();
    let mut shutdown = context.begin_shutdown();
    run_ready(shutdown.wait()).unwrap();
    assert_eq!(stops.load(Ordering::SeqCst), 1);
}

#[test]
fn test_shutdown_of_unmanaged_context_succeeds_without_actions() {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(First)).unwrap();
    let context = builder.build_all().unwrap();
    let mut shutdown = context.begin_shutdown();
    assert!(run_ready(shutdown.wait()).is_ok());
}

#[test]
fn test_managed_instance_registration_with_options_stops_on_explicit_shutdown() {
    let stops = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();
    let captured = Arc::clone(&stops);
    builder
        .register_managed_instance(Managed::new(Arc::new(First), move |_| {
            captured.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }))
        .unwrap();
    let captured = Arc::clone(&stops);
    builder
        .register_managed_instance_with(
            Managed::new(Arc::new(Second), move |_| {
                captured.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
            BindingOptions {
                id: Some("managed.second".to_owned()),
                ..Default::default()
            },
        )
        .unwrap();
    let context = builder.build_all().unwrap();

    let mut shutdown = context.begin_shutdown();
    run_ready(shutdown.wait()).unwrap();
    assert_eq!(stops.load(Ordering::SeqCst), 2);
}

#[test]
fn test_managed_async_factory_and_managed_instance_definition_are_supported() {
    let stops = Arc::new(AtomicUsize::new(0));
    let captured = Arc::clone(&stops);
    let mut builder = ContainerBuilder::new();
    builder
        .register_managed_async_factory::<First, _>(&[], move |_| {
            let captured = Arc::clone(&captured);
            Box::pin(async move {
                Ok(Managed::new(Arc::new(First), move |_| {
                    captured.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }))
            })
        })
        .unwrap();
    let source = DefinitionSource::new(
        "tests",
        "managed_lifecycle_tests",
        "managed_lifecycle_tests.rs",
        1,
        1,
        "ConcreteService",
    );
    let mut draft = DefinitionDraft::<ConcreteService>::from_managed_instance(
        source,
        Default::default(),
        Managed::new(Arc::new(ConcreteService), {
            let captured = Arc::clone(&stops);
            move |_| {
                captured.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }),
    )
    .unwrap();
    draft
        .bind::<dyn Service, _>(Default::default(), |concrete| -> Arc<dyn Service> { concrete })
        .unwrap();
    draft.register(&mut builder).unwrap();
    builder.root::<First>();
    builder.root::<dyn Service>();

    let context = run_ready(builder.build_all_async()).unwrap();
    let mut shutdown = context.begin_shutdown();
    run_ready(shutdown.wait()).unwrap();
    assert_eq!(stops.load(Ordering::SeqCst), 2);
}

#[test]
fn test_managed_async_definition_draft_constructs_and_shuts_down() {
    let stops = Arc::new(AtomicUsize::new(0));
    let captured = Arc::clone(&stops);
    let mut draft = DefinitionDraft::<ConcreteService>::new_managed_async(
        DefinitionSource::new(
            "tests",
            "managed_lifecycle_tests",
            "managed_lifecycle_tests.rs",
            1,
            1,
            "ConcreteService",
        ),
        &[],
        Default::default(),
        move |_| {
            let captured = Arc::clone(&captured);
            Box::pin(async move {
                Ok(Managed::new(Arc::new(ConcreteService), move |_| {
                    captured.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }))
            })
        },
    )
    .unwrap();
    draft
        .bind::<dyn Service, _>(Default::default(), |concrete| -> Arc<dyn Service> { concrete })
        .unwrap();
    let mut builder = ContainerBuilder::new();
    draft.register(&mut builder).unwrap();
    builder.root::<dyn Service>();

    let context = run_ready(builder.build_async()).unwrap();
    let mut shutdown = context.begin_shutdown();
    run_ready(shutdown.wait()).unwrap();
    assert_eq!(stops.load(Ordering::SeqCst), 1);
}

#[test]
fn test_dropping_context_does_not_stop_managed_components() {
    let stops = Arc::new(AtomicUsize::new(0));
    let captured = Arc::clone(&stops);
    let mut builder = ContainerBuilder::new();
    builder
        .register_managed_factory::<First, _>(&[], move |_| {
            Ok(Managed::new(Arc::new(First), move |_| {
                captured.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }))
        })
        .unwrap();
    let context = builder.build_all().unwrap();
    drop(context);
    assert_eq!(stops.load(Ordering::SeqCst), 0);
}
