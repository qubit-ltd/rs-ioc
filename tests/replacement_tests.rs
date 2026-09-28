// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

#![allow(clippy::result_large_err)]

use std::sync::Arc;

use qubit_ioc::__private::codegen_v1::DefinitionDraft;
use qubit_ioc::BindingKey;
use qubit_ioc::BindingOptions;
use qubit_ioc::BuildError;
use qubit_ioc::ContainerBuilder;
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

fn old_definition(builder: &mut ContainerBuilder) {
    let mut draft =
        DefinitionDraft::<Service>::from_instance(source("Service"), Default::default(), Arc::new(Service(1))).unwrap();
    draft.bind::<dyn Api, _>(Default::default(), |value| value).unwrap();
    draft
        .bind::<dyn LegacyApi, _>(Default::default(), |value| value)
        .unwrap();
    draft.register(builder).unwrap();
}

fn new_definition(builder: &mut ContainerBuilder) -> Result<(), RegistrationError> {
    let mut draft =
        DefinitionDraft::<Service>::from_instance(source("Service"), Default::default(), Arc::new(Service(2)))?;
    draft.bind::<dyn Api, _>(Default::default(), |value| value)?;
    draft.register(builder)
}

#[test]
fn test_replacement_replaces_the_complete_definition_and_its_aliases() {
    let mut builder = ContainerBuilder::new();
    old_definition(&mut builder);
    builder
        .replace_definition(BindingKey::of::<Service>(None), new_definition)
        .unwrap();
    builder.root::<dyn Api>();
    let context = builder.build().unwrap();
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
    assert_eq!(builder.build().unwrap().get::<dyn Api>().unwrap().value(), 1);
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
    let draft =
        DefinitionDraft::<Service>::from_instance(source("Service"), Default::default(), Arc::new(Service(1))).unwrap();
    draft.register(&mut builder).unwrap();
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
    assert_eq!(builder.build_all().unwrap().get::<Service>().unwrap().value(), 1);
}

#[test]
fn test_replacement_reports_ambiguous_active_originals_before_factories() {
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

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
        Err(BuildError::ReplacementOriginalAmbiguous { .. })
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
            let mut definition = DefinitionDraft::<Service>::from_instance(
                source("ServiceAgain"),
                Default::default(),
                Arc::new(Service(3)),
            )?;
            definition.bind::<dyn Api, _>(Default::default(), |value| value)?;
            definition.register(draft)
        })
        .unwrap();
    let context = builder.build_all().unwrap();
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
    assert!(matches!(builder.build(), Err(BuildError::MissingDependency { .. })));
}
