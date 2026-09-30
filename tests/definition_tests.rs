// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use qubit_ioc::BindingKey;
use qubit_ioc::BindingOptions;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Definition;
use qubit_ioc::DefinitionSource;
use qubit_ioc::Dependency;
use qubit_ioc::Managed;
use qubit_ioc::RegistrationError;
use qubit_ioc::WaitPolicy;
use tokio::runtime::Builder;

trait Repository: Send + Sync {
    fn value(&self) -> u32;
}

struct MemoryRepository;

impl Repository for MemoryRepository {
    fn value(&self) -> u32 {
        42
    }
}

/// Public aliases project the same concrete allocation.
#[test]
fn test_public_definition_alias_shares_one_concrete_instance() {
    let definition = Definition::<MemoryRepository>::builder()
        .instance(Arc::new(MemoryRepository))
        .bind::<dyn Repository, _>(BindingOptions::default(), |value| value)
        .build()
        .expect("valid definition");
    let mut builder = ContainerBuilder::new();
    builder.register_definition(definition).expect("register definition");
    let application = builder.build_all().expect("build definition");
    let context = application.context();
    let concrete = context.get::<MemoryRepository>().expect("concrete instance");
    let alias = context.get::<dyn Repository>().expect("alias instance");
    let projected: Arc<dyn Repository> = concrete;
    assert!(Arc::ptr_eq(&projected, &alias));
    assert_eq!(alias.value(), 42);
}

/// Stable explicit source for exact diagnostic assertions.
fn source() -> DefinitionSource {
    DefinitionSource::new(
        "tests",
        "definition_tests",
        "definition_tests.rs",
        1,
        1,
        "MemoryRepository",
    )
}

/// Every public validation error retains its exact context without callbacks.
#[test]
fn test_definition_validation_errors_preserve_source_and_do_not_execute_callbacks() {
    let error = Definition::<MemoryRepository>::builder()
        .source(source())
        .build()
        .err()
        .expect("missing source");
    match error {
        RegistrationError::MissingDefinitionFactory { definition } => assert_eq!(definition, source()),
        other => panic!("unexpected error: {other:?}"),
    }
    let calls = Arc::new(AtomicUsize::new(0));
    let captured = Arc::clone(&calls);
    let error = Definition::<MemoryRepository>::builder()
        .source(source())
        .factory(move |_| {
            captured.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(MemoryRepository))
        })
        .bind::<dyn Repository, _>(BindingOptions::default(), |value| value)
        .bind::<dyn Repository, _>(BindingOptions::default(), |_| panic!("validation must not project"))
        .build()
        .err()
        .expect("duplicate alias");
    match error {
        RegistrationError::DuplicateDefinitionKey { key, definition } => {
            assert_eq!(key, BindingKey::of::<dyn Repository>(None));
            assert_eq!(definition, source());
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let error = Definition::<MemoryRepository>::builder()
        .source(source())
        .instance(Arc::new(MemoryRepository))
        .binding(BindingOptions {
            profile: Some("prod".to_owned()),
            ..BindingOptions::default()
        })
        .bind::<dyn Repository, _>(
            BindingOptions {
                profile: Some("preview".to_owned()),
                ..BindingOptions::default()
            },
            |value| value,
        )
        .build()
        .err()
        .expect("profile mismatch");
    match error {
        RegistrationError::AliasProfileMismatch {
            definition,
            concrete,
            alias,
        } => {
            assert_eq!(definition, source());
            assert_eq!(concrete.as_deref(), Some("prod"));
            assert_eq!(alias.as_deref(), Some("preview"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
    let request = Dependency::of::<u32>();
    let error = Definition::<MemoryRepository>::builder()
        .source(source())
        .instance(Arc::new(MemoryRepository))
        .dependencies(&[request.clone(), request.clone()])
        .build()
        .err()
        .expect("duplicate dependency");
    match error {
        RegistrationError::DuplicateDependency { definition, dependency } => {
            assert_eq!(definition, source());
            assert_eq!(dependency, request);
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

/// Failed definition construction leaves previously staged roots intact.
#[test]
fn test_failed_definition_keeps_existing_factory_and_root() {
    let existing_calls = Arc::new(AtomicUsize::new(0));
    let failed_calls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();
    let captured = Arc::clone(&existing_calls);
    builder
        .register_factory::<u32, _>(&[], move |_| {
            captured.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(7))
        })
        .expect("existing definition");
    builder.root::<u32>();
    let captured = Arc::clone(&failed_calls);
    let definition = Definition::<MemoryRepository>::builder()
        .source(source())
        .factory(move |_| {
            captured.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(MemoryRepository))
        })
        .dependencies(&[Dependency::of::<u32>(), Dependency::of::<u32>()])
        .build();
    let result = match definition {
        Ok(definition) => builder.register_definition(definition),
        Err(error) => Err(error),
    };
    match result.expect_err("invalid definition") {
        RegistrationError::DuplicateDependency { dependency, definition } => {
            assert_eq!(dependency, Dependency::of::<u32>());
            assert_eq!(definition, source());
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(existing_calls.load(Ordering::SeqCst), 0);
    assert_eq!(failed_calls.load(Ordering::SeqCst), 0);
    let application = builder.build().expect("existing root still builds");
    let context = application.context();
    assert_eq!(*context.get::<u32>().expect("existing value"), 7);
    assert_eq!(existing_calls.load(Ordering::SeqCst), 1);
    assert_eq!(failed_calls.load(Ordering::SeqCst), 0);
}

/// Alias profiles inherit concrete options set after the alias declaration.
#[test]
fn test_alias_profile_inherits_final_concrete_options() {
    let definition = Definition::<MemoryRepository>::builder()
        .instance(Arc::new(MemoryRepository))
        .bind::<dyn Repository, _>(BindingOptions::default(), |value| value)
        .binding(BindingOptions {
            id: Some("first".to_owned()),
            profile: Some("preview".to_owned()),
            ..BindingOptions::default()
        })
        .binding(BindingOptions {
            id: Some("last".to_owned()),
            profile: Some("prod".to_owned()),
            ..BindingOptions::default()
        })
        .build()
        .expect("inherited profile");
    let mut builder = ContainerBuilder::new()
        .active_profiles(&["prod"])
        .expect("valid profile");
    builder.register_definition(definition).expect("valid definition");
    let application = builder.build_all().expect("inherited active aliases");
    let context = application.context();
    assert_eq!(context.get::<dyn Repository>().expect("active alias").value(), 42);
    let concrete = context
        .get_by_id::<MemoryRepository>("last")
        .expect("final concrete key");
    let projected: Arc<dyn Repository> = concrete;
    assert!(Arc::ptr_eq(
        &projected,
        &context.get::<dyn Repository>().expect("alias")
    ));
}

/// All five construction sources produce exactly one shared component.
#[test]
fn test_definition_supports_all_construction_sources_once() {
    for kind in 0..5 {
        let calls = Arc::new(AtomicUsize::new(0));
        let captured = Arc::clone(&calls);
        let definition = match kind {
            0 => {
                calls.fetch_add(1, Ordering::SeqCst);
                Definition::<u32>::builder().instance(Arc::new(42))
            }
            1 => Definition::<u32>::builder().factory(move |_| {
                captured.fetch_add(1, Ordering::SeqCst);
                Ok(Arc::new(42))
            }),
            2 => Definition::<u32>::builder().async_factory(move |_| {
                Box::pin(async move {
                    captured.fetch_add(1, Ordering::SeqCst);
                    Ok(Arc::new(42))
                })
            }),
            3 => Definition::<u32>::builder().managed_factory(move |_| {
                captured.fetch_add(1, Ordering::SeqCst);
                Ok(Managed::new(Arc::new(42), |_| Ok(())))
            }),
            4 => Definition::<u32>::builder().managed_async_factory(move |_| {
                Box::pin(async move {
                    captured.fetch_add(1, Ordering::SeqCst);
                    Ok(Managed::new(Arc::new(42), |_| Ok(())))
                })
            }),
            _ => unreachable!(),
        }
        .build()
        .expect("construction source");
        let mut builder = ContainerBuilder::new().wait_policy(WaitPolicy::unbounded());
        builder.register_definition(definition).expect("register source");
        assert_eq!(calls.load(Ordering::SeqCst), usize::from(kind == 0));
        let application = Builder::new_current_thread()
            .build()
            .expect("executor")
            .block_on(builder.build_all_async())
            .expect("build source");
        let context = application.context();
        assert_eq!(*context.get::<u32>().expect("component"), 42);
        assert!(Arc::ptr_eq(
            &context.get::<u32>().expect("component"),
            &context.get::<u32>().expect("same component")
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}

/// Default sources retain the external call site through convenience methods.
#[test]
fn test_definition_default_source_uses_caller() {
    let expected_line = line!() + 1;
    let definition = Definition::<u32>::builder()
        .instance(Arc::new(42))
        .build()
        .expect("definition");
    let mut builder = ContainerBuilder::new();
    builder.register_definition(definition).expect("register");
    let application = builder.build_all().expect("build");
    let context = application.context();
    let (actual, _) = context.binding_sources(&BindingKey::of::<u32>(None)).expect("source");
    assert_eq!(actual.file, file!());
    assert_eq!(actual.line, expected_line);
    let mut builder = ContainerBuilder::new();
    let expected_line = line!() + 1;
    builder.register_instance(Arc::new(42_u32)).expect("register");
    let application = builder.build_all().expect("build");
    let context = application.context();
    let (actual, _) = context.binding_sources(&BindingKey::of::<u32>(None)).expect("source");
    assert_eq!(actual.file, file!());
    assert_eq!(actual.line, expected_line);
}

/// Repeated setters use their last value while alias declarations accumulate.
#[test]
fn test_definition_setters_replace_previous_configuration() {
    let obsolete_calls = Arc::new(AtomicUsize::new(0));
    let captured = Arc::clone(&obsolete_calls);
    let final_source = DefinitionSource::new("tests", "final", "final.rs", 9, 3, "final");
    let definition = Definition::<u32>::builder()
        .source(source())
        .source(final_source)
        .dependencies(&[Dependency::of::<String>()])
        .dependencies(&[])
        .factory(move |_| {
            captured.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(1))
        })
        .instance(Arc::new(2))
        .factory(|_| Ok(Arc::new(3)))
        .build()
        .expect("last configuration wins");
    let mut builder = ContainerBuilder::new();
    builder.register_definition(definition).expect("register");
    let application = builder.build_all().expect("obsolete dependency removed");
    let context = application.context();
    assert_eq!(*context.get::<u32>().expect("final factory value"), 3);
    assert_eq!(obsolete_calls.load(Ordering::SeqCst), 0);
    let (actual, _) = context
        .binding_sources(&BindingKey::of::<u32>(None))
        .expect("final source");
    assert_eq!(actual, final_source);
}

/// Alias option validation rejects malformed profiles without projection.
#[test]
fn test_definition_rejects_invalid_alias_options_before_projection() {
    let error = Definition::<MemoryRepository>::builder()
        .source(source())
        .instance(Arc::new(MemoryRepository))
        .bind::<dyn Repository, _>(
            BindingOptions {
                profile: Some("9-invalid".to_owned()),
                ..BindingOptions::default()
            },
            |_| panic!("invalid alias must not project"),
        )
        .build()
        .err()
        .expect("invalid alias profile");
    match error {
        RegistrationError::InvalidProfile { value, definition } => {
            assert_eq!(value, "9-invalid");
            assert_eq!(definition, source());
        }
        other => panic!("unexpected error: {other:?}"),
    }
}
