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
use qubit_ioc::BindingOptions;
use qubit_ioc::BuildError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Dependency;
use qubit_ioc::FactoryError;
use qubit_ioc::RegistrationError;
#[cfg(feature = "config")]
use qubit_ioc::config::get_value_for;

#[derive(Debug)]
struct LevelOne(usize);
#[derive(Debug)]
struct LevelTwo(Arc<LevelOne>);
#[derive(Debug)]
struct LevelThree(Arc<LevelTwo>);

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

    let context = builder.build().expect("valid graph must build");
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
    builder
        .register_async_factory::<u64, _>(&[], move |_| {
            async_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(Arc::new(2)) })
        })
        .expect("stage asynchronous factory");

    assert!(matches!(builder.build(), Err(BuildError::AsyncRequired { .. })));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
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
    let mut future = pin!(builder.build_async());
    assert_send(&future);
    let waker = Waker::noop();
    let mut poll_context = Context::from_waker(waker);
    let context = match future.as_mut().poll(&mut poll_context) {
        Poll::Ready(result) => result.expect("ready factories must build"),
        Poll::Pending => panic!("ready factories unexpectedly yielded"),
    };
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
    let error = match builder.build() {
        Ok(_) => panic!("factory must fail"),
        Err(error) => error,
    };
    assert!(matches!(error, BuildError::FactoryFailed { .. }));
    let factory_error = error.source().expect("factory error source");
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

    let error = builder.build().err().expect("missing config value must fail");
    let message = error.to_string();
    assert!(
        message.contains("for port from"),
        "diagnostic must name the target: {message}"
    );
    assert!(
        message.contains("path:"),
        "diagnostic must show dependency path: {message}"
    );
    match &error {
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

    let error = match builder.build() {
        Ok(_) => panic!("dependency factory must fail"),
        Err(error) => error,
    };
    match error {
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

    let error = match builder.build() {
        Ok(_) => panic!("dependency factory must fail"),
        Err(error) => error,
    };
    match error {
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

    let mut future = Box::pin(builder.build_async());
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
        *builder.build().expect("valid graph").get::<u64>().expect("instance"),
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
            .build()
            .expect("inactive duplicate is valid")
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
            .build()
            .expect("rejected registrations leave no bindings")
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
    let context = builder.build().expect("build ordered collection");
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
    let error = ready(sync_builder.build_async())
        .err()
        .expect("sync failure must propagate from async build");
    assert!(matches!(&error, BuildError::FactoryFailed { .. }));
    assert!(
        error
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
    let error = ready(async_builder.build_async())
        .err()
        .expect("async failure must propagate from async build");
    assert!(matches!(&error, BuildError::FactoryFailed { .. }));
    assert!(
        error
            .source()
            .expect("factory wrapper")
            .source()
            .expect("domain source")
            .is::<DomainFailure>()
    );
}
