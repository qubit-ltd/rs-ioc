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

use qubit_ioc::BindingOptions;
use qubit_ioc::BuildError;
use qubit_ioc::CleanupError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Definition;
use qubit_ioc::DefinitionSource;
use qubit_ioc::Dependency;
use qubit_ioc::FactoryError;
use qubit_ioc::Managed;
use qubit_ioc::ShutdownMode;
use qubit_ioc::ShutdownPhase;
use qubit_ioc::WaitPolicy;

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
    Managed::asynchronous(
        Arc::new(value),
        move |_| {
            stop_events.lock().unwrap().push(name);
            Ok(())
        },
        move |_| {
            Box::pin(async move {
                wait_events.lock().unwrap().push(match name {
                    "stop A" => "wait A",
                    "stop B" => "wait B",
                    "stop C" => "wait C",
                    other => other,
                });
                Ok(())
            })
        },
    )
}

#[test]
fn test_shutdown_stops_then_waits_in_reverse_construction_order() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
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

    let application = builder.build_all().unwrap();

    assert!(events.lock().unwrap().is_empty());
    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);
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
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    let captured = Arc::clone(&stops);
    builder
        .register_managed_factory::<First, _>(&[], move |_| {
            Ok(Managed::synchronous(Arc::new(First), move |_| {
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

    let failure = builder.build_all().err().expect("factory must fail");
    assert!(matches!(failure.cause(), BuildError::FactoryFailed { .. }));
    assert_eq!(stops.load(Ordering::SeqCst), 1);
    assert_eq!(later_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn test_async_build_failure_defers_waits_and_reports_both_phases() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<First, _>(&[], move |_| {
            let stop_events = Arc::clone(&captured);
            let wait_events = Arc::clone(&captured);
            Ok(Managed::asynchronous(
                Arc::new(First),
                move |_| {
                    stop_events.lock().unwrap().push("stop");
                    Err(CleanupError::new(std::io::Error::other("stop failure")))
                },
                move |_| {
                    Box::pin(async move {
                        wait_events.lock().unwrap().push("wait");
                        Err(CleanupError::new(std::io::Error::other("wait failure")))
                    })
                },
            ))
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
    assert_eq!(*events.lock().unwrap(), ["stop"]);
    let (cause, cleanup) = error.into_parts();
    assert!(matches!(cause, BuildError::FactoryFailed { .. }));
    let mut cleanup = cleanup.expect("completed managed resource needs cleanup");
    let cleanup_error = run_ready(cleanup.wait()).expect_err("both cleanup phases fail");
    let failures = cleanup_error.report().failures();
    assert_eq!(failures.len(), 2);
    assert_eq!(failures[0].phase, ShutdownPhase::Abort);
    assert_eq!(failures[1].phase, ShutdownPhase::Wait);
    assert_eq!(*events.lock().unwrap(), ["stop", "wait"]);
}

#[test]
fn test_shutdown_continues_after_multiple_stop_and_wait_failures() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<First, _>(&[], move |_| {
            let stop_events = Arc::clone(&captured);
            let wait_events = Arc::clone(&captured);
            Ok(Managed::asynchronous(
                Arc::new(First),
                move |_| {
                    stop_events.lock().unwrap().push("stop A");
                    Err(CleanupError::new(std::io::Error::other("stop failure")))
                },
                move |_| {
                    Box::pin(async move {
                        wait_events.lock().unwrap().push("wait A");
                        Err(CleanupError::new(std::io::Error::other("wait failure")))
                    })
                },
            ))
        })
        .unwrap();
    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<Second, _>(&[Dependency::of::<First>()], move |context| {
            let _first = context.get::<First>().unwrap();
            let stop_events = Arc::clone(&captured);
            let wait_events = Arc::clone(&captured);
            Ok(Managed::asynchronous(
                Arc::new(Second),
                move |_| {
                    stop_events.lock().unwrap().push("stop B");
                    Err(CleanupError::new(std::io::Error::other("stop failure")))
                },
                move |_| {
                    Box::pin(async move {
                        wait_events.lock().unwrap().push("wait B");
                        Err(CleanupError::new(std::io::Error::other("wait failure")))
                    })
                },
            ))
        })
        .unwrap();

    let application = builder.build_all().unwrap();

    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);
    let error = run_ready(shutdown.wait()).expect_err("cleanup failures are returned");
    assert_eq!(error.report().failures().len(), 4);
    let source = std::error::Error::source(&error.report().failures()[0]).expect("cleanup error source");
    assert!(std::error::Error::source(source).is_some());
    let repeated = run_ready(shutdown.wait()).expect_err("completed errors are repeatable");
    assert_eq!(repeated.report().failures().len(), 4);
    assert!(std::ptr::eq(
        error.report().failures().as_ptr(),
        repeated.report().failures().as_ptr()
    ));
    assert_eq!(*events.lock().unwrap(), ["stop B", "stop A", "wait B", "wait A"]);
}

#[test]
fn test_shutdown_continues_after_stop_panic() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());

    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<First, _>(&[], move |_| {
            let stop_events = Arc::clone(&captured);
            let wait_events = Arc::clone(&captured);
            Ok(Managed::asynchronous(
                Arc::new(First),
                move |_| {
                    stop_events.lock().unwrap().push("stop A");
                    Ok(())
                },
                move |_| {
                    Box::pin(async move {
                        wait_events.lock().unwrap().push("wait A");
                        Ok(())
                    })
                },
            ))
        })
        .unwrap();

    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<Second, _>(&[Dependency::of::<First>()], move |context| {
            let _first = context.get::<First>().unwrap();
            let stop_events = Arc::clone(&captured);
            let wait_events = Arc::clone(&captured);
            Ok(Managed::asynchronous(
                Arc::new(Second),
                move |_| {
                    stop_events.lock().unwrap().push("stop B");
                    panic!("stop B panicked");
                },
                move |_| {
                    Box::pin(async move {
                        wait_events.lock().unwrap().push("wait B");
                        Ok(())
                    })
                },
            ))
        })
        .unwrap();

    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<Third, _>(&[Dependency::of::<Second>()], move |context| {
            let _second = context.get::<Second>().unwrap();
            let stop_events = Arc::clone(&captured);
            let wait_events = Arc::clone(&captured);
            Ok(Managed::asynchronous(
                Arc::new(Third),
                move |_| {
                    stop_events.lock().unwrap().push("stop C");
                    Ok(())
                },
                move |_| {
                    Box::pin(async move {
                        wait_events.lock().unwrap().push("wait C");
                        Ok(())
                    })
                },
            ))
        })
        .unwrap();

    let application = builder.build_all().unwrap();

    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);
    let error = run_ready(shutdown.wait()).expect_err("stop panic is reported");

    assert_eq!(error.report().failures().len(), 1);
    assert_eq!(error.report().failures()[0].phase, ShutdownPhase::Abort);
    assert_eq!(
        error.report().failures()[0].key.type_name(),
        std::any::type_name::<Second>()
    );
    let message = error.report().failures()[0].to_string();
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
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());

    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<First, _>(&[], move |_| {
            let stop_events = Arc::clone(&captured);
            let wait_events = Arc::clone(&captured);
            Ok(Managed::asynchronous(
                Arc::new(First),
                move |_| {
                    stop_events.lock().unwrap().push("stop A");
                    Ok(())
                },
                move |_| {
                    Box::pin(async move {
                        wait_events.lock().unwrap().push("wait A");
                        Ok(())
                    })
                },
            ))
        })
        .unwrap();

    let captured = Arc::clone(&events);
    builder
        .register_managed_factory::<Second, _>(&[], move |_| {
            let stop_events = Arc::clone(&captured);
            let wait_events = Arc::clone(&captured);
            Ok(Managed::asynchronous(
                Arc::new(Second),
                move |_| {
                    stop_events.lock().unwrap().push("stop B");
                    panic!("stop B panicked during build cleanup");
                },
                move |_| {
                    Box::pin(async move {
                        wait_events.lock().unwrap().push("wait B");
                        Ok(())
                    })
                },
            ))
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
    assert_eq!(*events.lock().unwrap(), ["stop B", "stop A"]);
    let (cause, cleanup) = error.into_parts();
    assert!(matches!(cause, BuildError::FactoryFailed { .. }));
    let mut cleanup = cleanup.expect("completed managed resources need cleanup");
    let cleanup_error = run_ready(cleanup.wait()).expect_err("abort panic remains observable");
    let failures = cleanup_error.report().failures();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].phase, ShutdownPhase::Abort);
    assert!(failures[0].to_string().contains("stop B panicked during build cleanup"));
    assert_eq!(*events.lock().unwrap(), ["stop B", "stop A", "wait B", "wait A"]);
}

#[test]
fn test_cancelled_shutdown_wait_resumes_the_same_future() {
    let callback_calls = Arc::new(AtomicUsize::new(0));
    let future_polls = Arc::new(AtomicUsize::new(0));
    let stops = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    let captured_calls = Arc::clone(&callback_calls);
    let captured_polls = Arc::clone(&future_polls);
    let captured_stops = Arc::clone(&stops);
    builder
        .register_managed_factory::<First, _>(&[], move |_| {
            Ok(Managed::asynchronous(
                Arc::new(First),
                move |_| {
                    captured_stops.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
                move |_| {
                    captured_calls.fetch_add(1, Ordering::SeqCst);
                    Box::pin(std::future::poll_fn(move |_| {
                        if captured_polls.fetch_add(1, Ordering::SeqCst) == 0 {
                            Poll::Pending
                        } else {
                            Poll::Ready(Ok(()))
                        }
                    }))
                },
            ))
        })
        .expect("register managed component");

    let application = builder.build_all().expect("build managed component");

    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);
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
        Poll::Ready(Ok(report)) if report.is_success()
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
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    let captured_stops = Arc::clone(&stops);
    let captured_waits = Arc::clone(&waits);
    builder
        .register_managed_factory::<First, _>(&[], move |_| {
            Ok(Managed::asynchronous(
                Arc::new(First),
                move |_| {
                    captured_stops.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
                move |_| {
                    Box::pin(async move {
                        captured_waits.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    })
                },
            ))
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
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    let captured_stops = Arc::clone(&stops);
    let captured_waits = Arc::clone(&waits);
    builder
        .register_managed_factory::<First, _>(&[], move |_| {
            Ok(Managed::asynchronous(
                Arc::new(First),
                move |_| {
                    captured_stops.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
                move |_| {
                    Box::pin(async move {
                        captured_waits.fetch_add(1, Ordering::SeqCst);
                        std::future::pending::<Result<(), CleanupError>>().await
                    })
                },
            ))
        })
        .unwrap();
    builder
        .register_async_factory::<Second, _>(&[], |_| {
            Box::pin(async { Err(FactoryError::new(std::io::Error::other("factory failure"))) })
        })
        .unwrap();

    let mut failure = run_ready(builder.build_all_async())
        .err()
        .expect("factory failure is immediate");
    assert_eq!(stops.load(Ordering::SeqCst), 1);
    assert_eq!(waits.load(Ordering::SeqCst), 0);
    let mut cleanup = failure.take_cleanup().expect("rollback owner");
    let mut future = Box::pin(cleanup.wait());
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
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    let captured = Arc::clone(&stops);
    let definition = Definition::<ConcreteService>::builder()
        .source(DefinitionSource::new(
            "tests",
            "managed_lifecycle_tests",
            "managed_lifecycle_tests.rs",
            1,
            1,
            "ConcreteService",
        ))
        .managed_factory(move |_| {
            Ok(Managed::synchronous(Arc::new(ConcreteService), move |_| {
                captured.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }))
        })
        .bind::<dyn Service, _>(Default::default(), |concrete| -> Arc<dyn Service> { concrete })
        .build()
        .unwrap();
    builder.register_definition(definition).unwrap();
    builder.root::<dyn Service>();

    let application = builder.build().unwrap();

    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);
    run_ready(shutdown.wait()).unwrap();
    assert_eq!(stops.load(Ordering::SeqCst), 1);
}

#[test]
fn test_shutdown_of_unmanaged_context_succeeds_without_actions() {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(First)).unwrap();
    let application = builder.build_all().unwrap();
    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);
    assert!(run_ready(shutdown.wait()).is_ok());
}

#[test]
fn test_unselected_managed_factory_is_not_invoked() {
    let constructions = Arc::new(AtomicUsize::new(0));
    let stops = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();
    let factory_constructions = Arc::clone(&constructions);
    let factory_stops = Arc::clone(&stops);
    builder
        .register_managed_factory::<First, _>(&[], move |_| {
            factory_constructions.fetch_add(1, Ordering::SeqCst);
            Ok(Managed::synchronous(Arc::new(First), move |_| {
                factory_stops.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }))
        })
        .expect("register managed factory");
    builder
        .register_instance(Arc::new(Second))
        .expect("register root instance");
    builder.root::<Second>();

    let application = builder.build().expect("build selected root");

    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);
    run_ready(shutdown.wait()).expect("shutdown succeeds");
    assert_eq!(constructions.load(Ordering::SeqCst), 0);
    assert_eq!(stops.load(Ordering::SeqCst), 0);
}

#[test]
fn test_graph_failure_does_not_create_managed_resource() {
    let constructions = Arc::new(AtomicUsize::new(0));
    let stops = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    let factory_constructions = Arc::clone(&constructions);
    let factory_stops = Arc::clone(&stops);
    builder
        .register_managed_factory::<First, _>(&[], move |_| {
            factory_constructions.fetch_add(1, Ordering::SeqCst);
            Ok(Managed::synchronous(Arc::new(First), move |_| {
                factory_stops.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }))
        })
        .expect("register managed factory");
    builder
        .register_factory::<Second, _>(&[Dependency::of::<Third>()], |_| Ok(Arc::new(Second)))
        .expect("register invalid root factory");
    builder.root::<Second>();

    let error = match builder.build() {
        Ok(_) => panic!("missing dependency prevents construction"),
        Err(error) => error,
    };
    assert!(matches!(error.cause(), BuildError::MissingDependency { .. }));
    assert_eq!(constructions.load(Ordering::SeqCst), 0);
    assert_eq!(stops.load(Ordering::SeqCst), 0);
}

#[test]
fn test_managed_factory_registration_with_options_stops_on_explicit_shutdown() {
    let stops = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    let captured = Arc::clone(&stops);
    builder
        .register_managed_factory::<First, _>(&[], move |_| {
            Ok(Managed::synchronous(Arc::new(First), move |_| {
                captured.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }))
        })
        .unwrap();
    let captured = Arc::clone(&stops);
    builder
        .register_managed_factory_with::<Second, _>(
            &[],
            BindingOptions {
                id: Some("managed.second".to_owned()),
                ..Default::default()
            },
            move |_| {
                Ok(Managed::synchronous(Arc::new(Second), move |_| {
                    captured.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }))
            },
        )
        .unwrap();
    let application = builder.build_all().unwrap();

    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);
    run_ready(shutdown.wait()).unwrap();
    assert_eq!(stops.load(Ordering::SeqCst), 2);
}

#[test]
fn test_managed_async_and_sync_definitions_are_supported() {
    let stops = Arc::new(AtomicUsize::new(0));
    let captured = Arc::clone(&stops);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_async_factory::<First, _>(&[], move |_| {
            let captured = Arc::clone(&captured);
            Box::pin(async move {
                Ok(Managed::synchronous(Arc::new(First), move |_| {
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
    let captured = Arc::clone(&stops);
    let definition = Definition::<ConcreteService>::builder()
        .source(source)
        .managed_factory(move |_| {
            Ok(Managed::synchronous(Arc::new(ConcreteService), move |_| {
                captured.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }))
        })
        .bind::<dyn Service, _>(Default::default(), |concrete| -> Arc<dyn Service> { concrete })
        .build()
        .unwrap();
    builder.register_definition(definition).unwrap();
    builder.root::<First>();
    builder.root::<dyn Service>();

    let application = run_ready(builder.build_all_async()).unwrap();

    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);
    run_ready(shutdown.wait()).unwrap();
    assert_eq!(stops.load(Ordering::SeqCst), 2);
}

#[test]
fn test_managed_async_definition_constructs_and_shuts_down() {
    let stops = Arc::new(AtomicUsize::new(0));
    let captured = Arc::clone(&stops);
    let definition = Definition::<ConcreteService>::builder()
        .source(DefinitionSource::new(
            "tests",
            "managed_lifecycle_tests",
            "managed_lifecycle_tests.rs",
            1,
            1,
            "ConcreteService",
        ))
        .managed_async_factory(move |_| {
            let captured = Arc::clone(&captured);
            Box::pin(async move {
                Ok(Managed::synchronous(Arc::new(ConcreteService), move |_| {
                    captured.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }))
            })
        })
        .bind::<dyn Service, _>(Default::default(), |concrete| -> Arc<dyn Service> { concrete })
        .build()
        .unwrap();
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder.register_definition(definition).unwrap();
    builder.root::<dyn Service>();

    let application = run_ready(builder.build_async()).unwrap();

    let mut shutdown = application.begin_shutdown(ShutdownMode::Immediate);
    run_ready(shutdown.wait()).unwrap();
    assert_eq!(stops.load(Ordering::SeqCst), 1);
}

#[test]
fn test_dropping_query_clone_does_not_stop_but_dropping_owner_stops_once() {
    let stops = Arc::new(AtomicUsize::new(0));
    let captured = Arc::clone(&stops);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_factory::<First, _>(&[], move |_| {
            Ok(Managed::synchronous(Arc::new(First), move |_| {
                captured.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }))
        })
        .unwrap();
    let application = builder.build_all().unwrap();
    let context = application.context().clone();
    drop(context);
    assert_eq!(stops.load(Ordering::SeqCst), 0);
    drop(application);
    assert_eq!(stops.load(Ordering::SeqCst), 1);
}

#[test]
fn test_explicit_drop_shutdown_stops_without_waiting() {
    let stops = Arc::new(AtomicUsize::new(0));
    let wait_callbacks = Arc::new(AtomicUsize::new(0));
    let waits = Arc::new(AtomicUsize::new(0));
    let stop_count = Arc::clone(&stops);
    let wait_callback_count = Arc::clone(&wait_callbacks);
    let wait_count = Arc::clone(&waits);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_factory::<First, _>(&[], move |_| {
            Ok(Managed::asynchronous(
                Arc::new(First),
                move |_| {
                    stop_count.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
                move |_| {
                    wait_callback_count.fetch_add(1, Ordering::SeqCst);
                    Box::pin(async move {
                        wait_count.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    })
                },
            ))
        })
        .expect("register managed component");

    drop(
        builder
            .build_all()
            .expect("build managed component")
            .begin_shutdown(ShutdownMode::Immediate),
    );
    assert_eq!(stops.load(Ordering::SeqCst), 1);
    assert_eq!(wait_callbacks.load(Ordering::SeqCst), 0);
    assert_eq!(waits.load(Ordering::SeqCst), 0);
}

#[test]
fn test_explicit_drop_untransferred_managed_aborts_once_without_starting_wait() {
    let stops = Arc::new(AtomicUsize::new(0));
    let waits = Arc::new(AtomicUsize::new(0));
    let stop_count = Arc::clone(&stops);
    let wait_count = Arc::clone(&waits);
    let managed = Managed::asynchronous(
        Arc::new(First),
        move |_| {
            stop_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        },
        move |_| {
            wait_count.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(()) })
        },
    );

    drop(managed);
    assert_eq!(stops.load(Ordering::SeqCst), 1);
    assert_eq!(waits.load(Ordering::SeqCst), 0);
}
