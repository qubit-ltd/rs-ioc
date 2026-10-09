// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use std::error::Error;
use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

#[cfg(feature = "config")]
use qubit_config::Config;
#[cfg(feature = "config")]
use qubit_config::ConfigError;
use qubit_ioc::BindingKey;
use qubit_ioc::BindingOptions;
use qubit_ioc::BuildError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Dependency;
use qubit_ioc::FactoryError;
use qubit_ioc::Managed;
use qubit_ioc::RegistrationError;
use qubit_ioc::ValidationScope;
use qubit_ioc::WaitPolicy;
#[cfg(feature = "config")]
use qubit_ioc::config::get_value_for;

#[derive(Debug)]
struct LevelOne(usize);
#[derive(Debug)]
struct LevelTwo(Arc<LevelOne>);
#[derive(Debug)]
struct LevelThree(Arc<LevelTwo>);

struct SettledResource;
struct SettledFailure;

struct SelectedRoot;
struct UnselectedWithMissingDependency;
struct MissingForUnselected;

/// Stages one valid root and one active definition with a missing dependency.
fn scoped_validation_builder(calls: &Arc<AtomicUsize>) -> ContainerBuilder {
    let mut builder = ContainerBuilder::new();
    let root_calls = Arc::clone(calls);
    builder
        .register_factory::<SelectedRoot, _>(&[], move |_| {
            root_calls.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(SelectedRoot))
        })
        .expect("stage selected root");
    let unselected_calls = Arc::clone(calls);
    builder
        .register_factory::<UnselectedWithMissingDependency, _>(
            &[Dependency::of::<MissingForUnselected>()],
            move |_| {
                unselected_calls.fetch_add(1, Ordering::SeqCst);
                Ok(Arc::new(UnselectedWithMissingDependency))
            },
        )
        .expect("stage invalid unselected definition");
    builder.root::<SelectedRoot>();
    builder
}

#[test]
fn test_reachable_validates_only_selected_closure() {
    let calls = Arc::new(AtomicUsize::new(0));
    let application = scoped_validation_builder(&calls)
        .build()
        .expect("default Reachable ignores the invalid unselected definition");
    assert!(application.context().get::<SelectedRoot>().is_ok());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn test_all_active_rejects_missing_unselected_dependency_before_any_factory() {
    let calls = Arc::new(AtomicUsize::new(0));
    let error = scoped_validation_builder(&calls)
        .validation_scope(ValidationScope::AllActive)
        .build()
        .err()
        .expect("AllActive validates the unselected definition");
    assert!(matches!(error.cause(), BuildError::MissingDependency { .. }));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("expected a ready future"),
    }
}

#[test]
fn test_build_settled_builds_selected_synchronous_graph() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_instance(Arc::new(42_u32))
        .expect("stage root instance");
    builder.root::<u32>();

    let application = ready(builder.build_settled()).expect("valid graph must build");
    assert_eq!(*application.context().get::<u32>().expect("built root"), 42);
}

#[test]
fn test_build_settled_reports_missing_root_without_cleanup() {
    let error = match ready(ContainerBuilder::new().build_settled()) {
        Ok(_) => panic!("missing root must fail"),
        Err(error) => error,
    };
    assert!(matches!(error.cause(), BuildError::NoRootsSelected));
    assert!(error.cleanup_report().is_none());
}

#[test]
fn test_build_async_settled_waits_for_cleanup_before_returning_failure() {
    let waits = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    let wait_count = Arc::clone(&waits);
    builder
        .register_managed_factory::<SettledResource, _>(&[], move |_| {
            Ok(Managed::asynchronous(
                Arc::new(SettledResource),
                |_| Ok(()),
                move |_| {
                    wait_count.fetch_add(1, Ordering::SeqCst);
                    Box::pin(async { Ok(()) })
                },
            ))
        })
        .expect("stage managed resource");
    builder
        .register_factory::<SettledFailure, _>(&[Dependency::of::<SettledResource>()], |_| {
            Err(FactoryError::new(std::io::Error::other("expected failure")))
        })
        .expect("stage failing factory");
    builder.root::<SettledFailure>();

    let error = match ready(builder.build_async_settled()) {
        Ok(_) => panic!("later factory must fail"),
        Err(error) => error,
    };
    assert!(matches!(error.cause(), BuildError::FactoryFailed { .. }));
    assert_eq!(waits.load(Ordering::SeqCst), 1);
}

#[test]
fn test_build_all_async_settled_waits_for_managed_rollback() {
    let aborts = Arc::new(AtomicUsize::new(0));
    let waits = Arc::new(AtomicUsize::new(0));
    let abort_count = Arc::clone(&aborts);
    let wait_count = Arc::clone(&waits);
    let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
    builder
        .register_managed_factory::<SettledResource, _>(&[], move |_| {
            Ok(Managed::asynchronous(
                Arc::new(SettledResource),
                move |_| {
                    abort_count.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
                move |_| {
                    wait_count.fetch_add(1, Ordering::SeqCst);
                    Box::pin(async { Ok(()) })
                },
            ))
        })
        .expect("stage managed resource");
    builder
        .register_async_factory::<SettledFailure, _>(&[Dependency::of::<SettledResource>()], |_| {
            Box::pin(async { Err(FactoryError::new(std::io::Error::other("expected failure"))) })
        })
        .expect("stage failing async factory");

    let failure = match ready(builder.build_all_async_settled()) {
        Ok(_) => panic!("later async factory must fail"),
        Err(failure) => failure,
    };
    assert!(matches!(failure.cause(), BuildError::FactoryFailed { .. }));
    assert!(failure.cleanup_report().expect("rollback report").is_success());
    assert_eq!(aborts.load(Ordering::SeqCst), 1);
    assert_eq!(waits.load(Ordering::SeqCst), 1);
}

#[test]
fn test_build_all_settled_and_async_settled_accept_empty_graph() {
    let application = ready(ContainerBuilder::new().build_all_settled()).expect("empty synchronous graph must build");
    drop(application);

    let application =
        ready(ContainerBuilder::new().build_all_async_settled()).expect("empty asynchronous graph must build");
    drop(application);
}

#[test]
fn test_build_resolves_reverse_registered_chain() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<LevelThree, _>(&[Dependency::of::<LevelTwo>()], |context| {
            Ok(Arc::new(LevelThree(
                context.get::<LevelTwo>().expect("declared level two"),
            )))
        })
        .expect("stage level three");
    builder
        .register_factory::<LevelTwo, _>(&[Dependency::of::<LevelOne>()], |context| {
            Ok(Arc::new(LevelTwo(
                context.get::<LevelOne>().expect("declared level one"),
            )))
        })
        .expect("stage level two");
    builder
        .register_instance(Arc::new(LevelOne(42)))
        .expect("stage level one");

    let application = builder.build_all().expect("valid graph must build");

    let context = application.context();
    assert_eq!(context.get::<LevelThree>().expect("built level three").0.0.0, 42);
}

#[test]
fn test_build_requires_async_before_running_any_factory() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();
    let sync_calls = Arc::clone(&calls);
    builder
        .register_factory::<u32, _>(&[], move |_| {
            sync_calls.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(1))
        })
        .expect("stage synchronous factory");
    let async_calls = Arc::clone(&calls);
    let async_line = line!() + 2;
    builder
        .register_async_factory::<u64, _>(&[], move |_| {
            async_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(Arc::new(2)) })
        })
        .expect("stage asynchronous factory");
    builder
        .register_async_factory::<u8, _>(&[], |_| Box::pin(async { Ok(Arc::new(3)) }))
        .expect("stage later asynchronous factory");

    let error = builder.build_all().err().expect("synchronous build requires async");
    assert!(matches!(error.cause(), BuildError::AsyncRequired { definition, key }
        if key == &BindingKey::of::<u64>(None)
            && definition.file == file!()
            && definition.line == async_line
            && definition.item == std::any::type_name::<u64>()));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn test_build_sync_root_ignores_unselected_async_factory() {
    let sync_calls = Arc::new(AtomicUsize::new(0));
    let async_calls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();
    let captured = Arc::clone(&sync_calls);
    builder
        .register_factory::<u32, _>(&[], move |_| {
            captured.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(1))
        })
        .expect("stage selected synchronous factory");
    let captured = Arc::clone(&async_calls);
    builder
        .register_async_factory::<u64, _>(&[], move |_| {
            captured.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(Arc::new(2)) })
        })
        .expect("stage unselected asynchronous factory");
    builder.root::<u32>();

    let application = builder.build().expect("selected graph only requires sync");

    let context = application.context();
    assert_eq!(*context.get::<u32>().expect("selected synchronous value"), 1);
    assert!(context.get_all::<u64>().is_empty());
    assert_eq!(sync_calls.load(Ordering::SeqCst), 1);
    assert_eq!(async_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn test_build_async_mixes_factory_kinds() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<u32, _>(&[], |_| Ok(Arc::new(3)))
        .expect("stage synchronous factory");
    builder
        .register_async_factory::<u64, _>(&[Dependency::of::<u32>()], |context| {
            Box::pin(async move {
                let value = context.get::<u32>().expect("declared dependency");
                Ok(Arc::new(u64::from(*value) + 4))
            })
        })
        .expect("stage asynchronous factory");

    fn assert_send<T: Send>(_: &T) {}
    let mut future = pin!(builder.build_all_async());
    assert_send(&future);
    let waker = Waker::noop();
    let mut poll_context = Context::from_waker(waker);
    let application = match future.as_mut().poll(&mut poll_context) {
        Poll::Ready(result) => result.expect("ready factories must build"),
        Poll::Pending => panic!("ready factories unexpectedly yielded"),
    };
    let context = application.context();
    assert_eq!(*context.get::<u64>().expect("built async value"), 7);
}

#[derive(Debug, thiserror::Error)]
#[error("domain failure")]
struct DomainFailure;

#[test]
fn test_build_retains_factory_source_chain() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<u32, _>(&[], |_| Err(FactoryError::new(DomainFailure)))
        .expect("stage failing factory");
    let error = match builder.build_all() {
        Ok(_) => panic!("factory must fail"),
        Err(error) => error,
    };
    assert!(matches!(error.cause(), BuildError::FactoryFailed { .. }));
    let factory_error = error.cause().source().expect("factory error source");
    assert!(factory_error.source().expect("domain source").is::<DomainFailure>());
}

#[cfg(feature = "config")]
#[test]
fn test_build_maps_generated_config_error_with_complete_path() {
    let mut builder = ContainerBuilder::new()
        .with_config(Config::new())
        .expect("register an empty configuration snapshot");
    builder
        .register_factory::<u64, _>(&[Dependency::of::<u32>()], |context| {
            let value = context.get::<u32>().expect("declared value dependency");
            Ok(Arc::new(u64::from(*value)))
        })
        .expect("stage consumer");
    builder
        .register_factory::<u32, _>(&[Dependency::of::<Config>()], |context| {
            let config = context.get::<Config>().expect("declared config dependency");
            get_value_for::<u32>(&config, "service.port", "port").map(Arc::new)
        })
        .expect("stage generated-style read");

    let error = builder.build_all().err().expect("missing config value must fail");
    let message = error.to_string();
    assert!(
        message.contains("for port from"),
        "diagnostic must name the target: {message}"
    );
    assert!(
        message.contains("path:"),
        "diagnostic must show dependency path: {message}"
    );
    match error.cause() {
        BuildError::ConfigReadFailed {
            path_key, target, path, ..
        } => {
            assert_eq!(path_key, "service.port");
            assert_eq!(target, "port");
            assert_eq!(path.len(), 2);
            assert_eq!(path[0].type_name(), std::any::type_name::<u64>());
            assert_eq!(path[1].type_name(), std::any::type_name::<u32>());
        }
        other => panic!("expected ConfigReadFailed, got {other:?}"),
    }
    let source = error
        .cause()
        .source()
        .expect("factory wrapper")
        .source()
        .expect("original config error");
    assert!(matches!(source.downcast_ref::<ConfigError>(),
        Some(ConfigError::PropertyNotFound(path)) if path == "service.port"));
}

#[test]
fn test_build_failure_carries_consumer_to_dependency_path() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<u64, _>(&[Dependency::of::<u32>()], |context| {
            Ok(Arc::new(u64::from(*context.get::<u32>().expect("declared dependency"))))
        })
        .expect("stage consumer first");
    builder
        .register_factory::<u32, _>(&[], |_| Err(FactoryError::new(DomainFailure)))
        .expect("stage failing dependency");

    let error = match builder.build_all() {
        Ok(_) => panic!("dependency factory must fail"),
        Err(error) => error,
    };
    match error.cause() {
        BuildError::FactoryFailed { path, .. } => {
            assert_eq!(path.len(), 2);
            assert_eq!(path[0].type_name(), std::any::type_name::<u64>());
            assert_eq!(path[1].type_name(), std::any::type_name::<u32>());
        }
        other => panic!("unexpected build error: {other}"),
    }
}

#[test]
fn test_build_failure_path_includes_later_registered_consumer() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<u32, _>(&[], |_| Err(FactoryError::new(DomainFailure)))
        .expect("stage failing dependency first");
    builder
        .register_factory::<u64, _>(&[Dependency::of::<u32>()], |context| {
            Ok(Arc::new(u64::from(*context.get::<u32>().expect("declared dependency"))))
        })
        .expect("stage consumer second");

    let error = match builder.build_all() {
        Ok(_) => panic!("dependency factory must fail"),
        Err(error) => error,
    };
    match error.cause() {
        BuildError::FactoryFailed { path, .. } => {
            assert_eq!(path.len(), 2);
            assert_eq!(path[0].type_name(), std::any::type_name::<u64>());
            assert_eq!(path[1].type_name(), std::any::type_name::<u32>());
        }
        other => panic!("unexpected build error: {other}"),
    }
}

#[test]
fn test_build_async_cancellation_does_not_start_later_factory() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();
    builder
        .register_async_factory::<u32, _>(&[], |_| {
            Box::pin(async {
                std::future::pending::<()>().await;
                Ok(Arc::new(1))
            })
        })
        .expect("stage pending factory");
    let later_calls = Arc::clone(&calls);
    builder
        .register_factory::<u64, _>(&[], move |_| {
            later_calls.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(2))
        })
        .expect("stage later factory");

    let mut future = Box::pin(builder.build_all_async());
    let waker = Waker::noop();
    let mut poll_context = Context::from_waker(waker);
    assert!(future.as_mut().poll(&mut poll_context).is_pending());
    drop(future);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn test_register_factory_rejects_duplicate_requests_atomically() {
    let mut builder = ContainerBuilder::new();
    let duplicated = [Dependency::of::<u32>(), Dependency::of::<u32>()];
    assert!(matches!(
        builder.register_factory::<u64, _>(&duplicated, |_| Ok(Arc::new(1))),
        Err(RegistrationError::DuplicateDependency { .. })
    ));
    builder
        .register_instance(Arc::new(2_u64))
        .expect("failed definition left no binding");
    assert_eq!(
        *builder
            .build_all()
            .expect("valid graph")
            .context()
            .get::<u64>()
            .expect("instance"),
        2
    );
}

#[test]
fn test_build_filters_inactive_profile_before_duplicate_check() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_instance(Arc::new(1_u32))
        .expect("stage default instance");
    builder
        .register_instance_with(
            Arc::new(2_u32),
            BindingOptions {
                profile: Some("production".into()),
                ..BindingOptions::default()
            },
        )
        .expect("stage inactive instance");
    assert_eq!(
        *builder
            .build_all()
            .expect("inactive duplicate is valid")
            .context()
            .get::<u32>()
            .expect("active instance"),
        1
    );
}

#[test]
fn test_invalid_registration_id_and_profile_return_structured_errors() {
    let mut builder = ContainerBuilder::new();
    let error = builder
        .register_instance_with(
            Arc::new(1_u32),
            BindingOptions {
                id: Some("invalid-id".to_owned()),
                ..BindingOptions::default()
            },
        )
        .expect_err("invalid binding ID must fail at registration");
    assert!(matches!(error, RegistrationError::InvalidBindingId { error, .. }
        if error.value() == "invalid-id"));

    let error = builder
        .register_factory::<u64, _>(&[Dependency::with_id::<u32>("invalid-id")], |_| Ok(Arc::new(1)))
        .expect_err("invalid dependency ID must fail at registration");
    assert!(matches!(error, RegistrationError::InvalidBindingId { error, .. }
        if error.value() == "invalid-id"));
    assert!(
        builder
            .build_all()
            .expect("rejected registrations leave no bindings")
            .context()
            .get_all::<u32>()
            .is_empty()
    );

    let error = ContainerBuilder::new()
        .active_profiles(&["bad profile"])
        .err()
        .expect("invalid profile must fail");
    assert!(matches!(error, RegistrationError::InvalidProfile { value, .. }
        if value == "bad profile"));
}

#[test]
fn test_collection_dependency_uses_id_order_when_priorities_match() {
    let mut builder = ContainerBuilder::new();
    for (id, value) in [("zeta", 26_u32), ("alpha", 1_u32)] {
        builder
            .register_instance_with(
                Arc::new(value),
                BindingOptions {
                    id: Some(id.to_owned()),
                    ..BindingOptions::default()
                },
            )
            .expect("stage candidate");
    }
    builder
        .register_factory::<Vec<u32>, _>(&[Dependency::all::<u32>()], |context| {
            Ok(Arc::new(
                context
                    .get_all::<u32>()
                    .expect("declared collection")
                    .iter()
                    .map(|value| **value)
                    .collect(),
            ))
        })
        .expect("stage collection consumer");
    let application = builder.build_all().expect("build ordered collection");
    let context = application.context();
    assert_eq!(
        context.get::<Vec<u32>>().expect("collection result").as_slice(),
        &[1, 26]
    );
}

#[test]
fn test_build_async_preserves_both_sync_and_async_factory_failures() {
    /// Drives a ready factory future using a no-op waker.
    fn ready<F: Future>(future: F) -> F::Output {
        let mut future = pin!(future);
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("test factory unexpectedly suspended"),
        }
    }

    let mut sync_builder = ContainerBuilder::new();
    sync_builder
        .register_factory::<u32, _>(&[], |_| Err(FactoryError::new(DomainFailure)))
        .expect("stage failing sync factory");
    let error = ready(sync_builder.build_all_async())
        .err()
        .expect("sync failure must propagate from async build");
    assert!(matches!(error.cause(), BuildError::FactoryFailed { .. }));
    assert!(
        error
            .cause()
            .source()
            .expect("factory wrapper")
            .source()
            .expect("domain source")
            .is::<DomainFailure>()
    );

    let mut async_builder = ContainerBuilder::new();
    async_builder
        .register_async_factory::<u64, _>(&[], |_| Box::pin(async { Err(FactoryError::new(DomainFailure)) }))
        .expect("stage failing async factory");
    let error = ready(async_builder.build_all_async())
        .err()
        .expect("async failure must propagate from async build");
    assert!(matches!(error.cause(), BuildError::FactoryFailed { .. }));
    assert!(
        error
            .cause()
            .source()
            .expect("factory wrapper")
            .source()
            .expect("domain source")
            .is::<DomainFailure>()
    );
}
