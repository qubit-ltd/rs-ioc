// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

#![allow(clippy::result_large_err)]

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use qubit_ioc::BindingKey;
use qubit_ioc::BindingOptions;
use qubit_ioc::BuildError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Definition;
use qubit_ioc::Dependency;
use qubit_ioc::RegistrationError;
use qubit_ioc::options::DefinitionSource;

trait Api: Send + Sync {
    fn value(&self) -> u32;
}

trait LegacyApi: Send + Sync {}

impl LegacyApi for Service {}

struct Service(u32);

impl Api for Service {
    fn value(&self) -> u32 {
        self.0
    }
}

fn source(name: &'static str) -> DefinitionSource {
    DefinitionSource::new("tests", "replacement_tests", "replacement_tests.rs", 1, 1, name)
}

/// Registers the original complete definition and its two aliases.
fn old_definition(builder: &mut ContainerBuilder) {
    let definition = Definition::<Service>::builder()
        .source(source("Service"))
        .instance(Arc::new(Service(1)))
        .bind::<dyn Api, _>(Default::default(), |value| value)
        .bind::<dyn LegacyApi, _>(Default::default(), |value| value)
        .build()
        .expect("valid original definition");
    builder.register_definition(definition).expect("register original");
}

/// Registers a replacement with one retained interface alias.
fn new_definition(builder: &mut ContainerBuilder) -> Result<(), RegistrationError> {
    let definition = Definition::<Service>::builder()
        .source(source("Service"))
        .instance(Arc::new(Service(2)))
        .bind::<dyn Api, _>(Default::default(), |value| value)
        .build()?;
    builder.register_definition(definition)
}

#[test]
fn test_replacement_replaces_the_complete_definition_and_its_aliases() {
    let mut builder = ContainerBuilder::new();
    old_definition(&mut builder);
    builder
        .replace_definition(BindingKey::of::<Service>(None), new_definition)
        .unwrap();
    builder.root::<dyn Api>();
    let application = builder.build().unwrap();
    let context = application.context();
    let interface = context.get::<dyn Api>().unwrap();
    let concrete = context.get::<Service>().unwrap();
    let (active, replaced) = context
        .binding_sources(&BindingKey::of::<Service>(None))
        .expect("replacement sources are retained");
    assert_eq!(active.item, "Service");
    assert_eq!(
        replaced.iter().map(|source| source.item).collect::<Vec<_>>(),
        ["Service"]
    );
    assert_eq!(interface.value(), 2);
    assert_eq!(concrete.value(), 2);
    assert!(std::ptr::eq(
        Arc::as_ptr(&interface) as *const (),
        Arc::as_ptr(&concrete) as *const ()
    ));
}

#[test]
fn test_replacement_callback_errors_and_invalid_drafts_leave_builder_unchanged() {
    let mut builder = ContainerBuilder::new();
    old_definition(&mut builder);
    let error = builder
        .replace_definition(BindingKey::of::<Service>(None), |_| {
            Err(RegistrationError::EmptyDefinition {
                definition: source("empty"),
            })
        })
        .unwrap_err();
    assert!(matches!(error, RegistrationError::EmptyDefinition { .. }));
    let error = builder
        .replace_definition(BindingKey::of::<Service>(None), |draft| {
            draft.register_instance(Arc::new(4_u32))
        })
        .unwrap_err();
    assert!(matches!(
        error,
        RegistrationError::ReplacementAnchorCount { count: 0, .. }
    ));
    builder.root::<dyn Api>();
    assert_eq!(builder.build().unwrap().context().get::<dyn Api>().unwrap().value(), 1);
}

#[test]
fn test_replacement_requires_exactly_one_staged_definition() {
    let mut builder = ContainerBuilder::new();
    let error = builder
        .replace_definition(BindingKey::of::<u32>(None), |draft| {
            draft.register_instance(Arc::new(1_u32))?;
            draft.register_instance(Arc::new(2_u64))
        })
        .unwrap_err();
    assert!(matches!(
        error,
        RegistrationError::ReplacementDefinitionCount { count: 2 }
    ));
}

#[test]
fn test_inactive_replacement_keeps_the_original_definition() {
    let mut builder = ContainerBuilder::new();
    let definition = Definition::<Service>::builder()
        .source(source("Service"))
        .instance(Arc::new(Service(1)))
        .build()
        .unwrap();
    builder.register_definition(definition).unwrap();
    builder
        .replace_definition(BindingKey::of::<Service>(None), |draft| {
            draft.register_instance_with(
                Arc::new(Service(2)),
                BindingOptions {
                    profile: Some("preview".to_owned()),
                    ..Default::default()
                },
            )
        })
        .unwrap();
    assert_eq!(
        builder.build_all().unwrap().context().get::<Service>().unwrap().value(),
        1
    );
}

#[test]
fn test_replacement_uses_default_and_explicit_profile_selection() {
    let cases: [(&[&str], &str, u32); 4] = [
        (&[], "default", 2),
        (&[], "prod", 1),
        (&["prod"], "default", 1),
        (&["prod"], "prod", 2),
    ];
    for (active_profiles, replacement_profile, expected) in cases {
        let mut builder = ContainerBuilder::new()
            .active_profiles(active_profiles)
            .expect("valid active profiles");
        let original = Definition::<Service>::builder()
            .source(source("Original"))
            .instance(Arc::new(Service(1)))
            .build()
            .expect("stage unprofiled original");
        builder.register_definition(original).expect("register original");
        builder
            .replace_definition(BindingKey::of::<Service>(None), |draft| {
                let replacement = Definition::<Service>::builder()
                    .source(source("Replacement"))
                    .binding(BindingOptions {
                        profile: Some(replacement_profile.to_owned()),
                        ..Default::default()
                    })
                    .instance(Arc::new(Service(2)))
                    .build()?;
                draft.register_definition(replacement)
            })
            .expect("stage profiled replacement");

        let application = builder.build_all().expect("profile selection permits build");

        let context = application.context();
        assert_eq!(
            context.get::<Service>().expect("active service").value(),
            expected,
            "replacement {replacement_profile}, active {active_profiles:?}"
        );
        let (active, replaced) = context
            .binding_sources(&BindingKey::of::<Service>(None))
            .expect("active source is retained");
        if expected == 1 {
            assert_eq!(active, source("Original"));
            assert!(replaced.is_empty());
        } else {
            assert_eq!(active, source("Replacement"));
            assert_eq!(replaced, &[source("Original")]);
        }
    }
}

#[test]
fn test_replacement_reports_ambiguous_active_originals_before_factories() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(1_u64)).unwrap();
    builder.register_instance(Arc::new(2_u64)).unwrap();
    let captured = Arc::clone(&calls);
    builder
        .replace_definition(BindingKey::of::<u64>(None), move |draft| {
            draft.register_factory::<u64, _>(&[], move |_| {
                captured.fetch_add(1, Ordering::SeqCst);
                Ok(Arc::new(3))
            })
        })
        .unwrap();
    assert!(matches!(
        builder.build_all(),
        Err(failure) if matches!(failure.cause(), BuildError::ReplacementOriginalAmbiguous { .. })
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn test_consecutive_replacements_preserve_all_original_sources() {
    let mut builder = ContainerBuilder::new();
    old_definition(&mut builder);
    builder
        .replace_definition(BindingKey::of::<Service>(None), new_definition)
        .unwrap();
    builder
        .replace_definition(BindingKey::of::<Service>(None), |draft| {
            let definition = Definition::<Service>::builder()
                .source(source("ServiceAgain"))
                .instance(Arc::new(Service(3)))
                .bind::<dyn Api, _>(Default::default(), |value| value)
                .build()?;
            draft.register_definition(definition)
        })
        .unwrap();
    let application = builder.build_all().unwrap();
    let context = application.context();
    assert_eq!(context.get::<Service>().unwrap().value(), 3);
    let (active, replaced) = context.binding_sources(&BindingKey::of::<Service>(None)).unwrap();
    assert_eq!(active.item, "ServiceAgain");
    assert_eq!(
        replaced.iter().map(|source| source.item).collect::<Vec<_>>(),
        ["Service", "Service"]
    );
}

#[test]
fn test_replacement_removes_old_aliases_and_dependencies_report_missing_alias() {
    let mut builder = ContainerBuilder::new();
    old_definition(&mut builder);
    builder
        .replace_definition(BindingKey::of::<Service>(None), new_definition)
        .unwrap();
    builder
        .register_factory::<u32, _>(&[Dependency::of::<dyn LegacyApi>()], |_| Ok(Arc::new(1)))
        .unwrap();
    builder.root::<u32>();
    assert!(matches!(builder.build(), Err(failure) if matches!(failure.cause(), BuildError::MissingDependency { .. })));
}
