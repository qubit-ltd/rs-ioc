// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
#![cfg(feature = "inventory")]

use std::sync::Arc;
use std::sync::Mutex;

use qubit_ioc::__private::codegen_v1::DefinitionDraft;
use qubit_ioc::BindingId;
use qubit_ioc::BindingKey;
use qubit_ioc::BindingOptions;
use qubit_ioc::BuildError;
use qubit_ioc::ComponentDefinition;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::DefinitionSource;
use qubit_ioc::Dependency;
use qubit_ioc::RegistrationError;
use qubit_ioc::discovery::RegistrationEntry;

static TEST_LOCK: Mutex<()> = Mutex::new(());
static CONSTRUCTION_ORDER: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
const FIRST_SOURCE: DefinitionSource =
    DefinitionSource::new("test", "discovery_tests", "discovery_tests.rs", 10, 1, "SortedFirst");
const SECOND_SOURCE: DefinitionSource =
    DefinitionSource::new("test", "discovery_tests", "discovery_tests.rs", 20, 1, "SortedSecond");
const MEMORY_SOURCE: DefinitionSource = DefinitionSource::new(
    "test",
    "discovery_tests",
    "discovery_tests.rs",
    30,
    1,
    "MemoryRepository",
);
const SHARED_SOURCE: DefinitionSource =
    DefinitionSource::new("test", "discovery_tests", "discovery_tests.rs", 50, 1, "shared");

struct SortedFirst;
struct SortedSecond;

impl ComponentDefinition for SortedFirst {
    fn source() -> DefinitionSource {
        FIRST_SOURCE
    }
    fn definition_id() -> &'static str {
        "test::discovery_tests::SortedFirst"
    }

    fn register(builder: &mut ContainerBuilder) -> Result<(), RegistrationError> {
        let draft = DefinitionDraft::<SortedFirst>::new_sync(Self::source(), &[], BindingOptions::default(), |_| {
            CONSTRUCTION_ORDER.lock().expect("order mutex").push("first");
            Ok(Arc::new(SortedFirst))
        })?;
        draft.register(builder)
    }
}

impl ComponentDefinition for SortedSecond {
    fn source() -> DefinitionSource {
        SECOND_SOURCE
    }
    fn definition_id() -> &'static str {
        "test::discovery_tests::SortedSecond"
    }

    fn register(builder: &mut ContainerBuilder) -> Result<(), RegistrationError> {
        let draft = DefinitionDraft::<SortedSecond>::new_sync(Self::source(), &[], BindingOptions::default(), |_| {
            CONSTRUCTION_ORDER.lock().expect("order mutex").push("second");
            Ok(Arc::new(SortedSecond))
        })?;
        draft.register(builder)
    }
}

// Deliberately submitted in reverse source order.
qubit_ioc::__private::submit_component!(RegistrationEntry::new(
    SortedSecond::register,
    SECOND_SOURCE,
    "test::discovery_tests::SortedSecond",
));
qubit_ioc::__private::submit_component!(RegistrationEntry::new(
    SortedFirst::register,
    FIRST_SOURCE,
    "test::discovery_tests::SortedFirst",
));

trait Repository: Send + Sync {
    fn label(&self) -> &'static str;
}

struct MemoryRepository;
impl Repository for MemoryRepository {
    fn label(&self) -> &'static str {
        "memory"
    }
}

impl ComponentDefinition for MemoryRepository {
    fn source() -> DefinitionSource {
        MEMORY_SOURCE
    }
    fn definition_id() -> &'static str {
        "test::discovery_tests::MemoryRepository"
    }

    fn register(builder: &mut ContainerBuilder) -> Result<(), RegistrationError> {
        let options = BindingOptions {
            id: Some("example.repository".into()),
            ..BindingOptions::default()
        };
        let mut draft = DefinitionDraft::<MemoryRepository>::new_sync(Self::source(), &[], options.clone(), |_| {
            Ok(Arc::new(MemoryRepository))
        })?;
        draft.bind::<dyn Repository, _>(options, |concrete| concrete)?;
        draft.register(builder)
    }
}

/// Delegates to the component without sharing its function pointer identity.
// The public RegistrationError preserves complete binding and source metadata.
#[allow(clippy::result_large_err)]
fn register_memory_repository(builder: &mut ContainerBuilder) -> Result<(), RegistrationError> {
    MemoryRepository::register(builder)
}

qubit_ioc::__private::submit_component!(RegistrationEntry::new(
    register_memory_repository,
    MEMORY_SOURCE,
    "test::discovery_tests::MemoryRepository",
));

struct ReplacementRepository;
impl Repository for ReplacementRepository {
    fn label(&self) -> &'static str {
        "replacement"
    }
}

impl ComponentDefinition for ReplacementRepository {
    fn source() -> DefinitionSource {
        DefinitionSource::new(
            "test",
            "discovery_tests",
            "discovery_tests.rs",
            40,
            1,
            "ReplacementRepository",
        )
    }
    fn definition_id() -> &'static str {
        "test::discovery_tests::ReplacementRepository"
    }

    fn register(builder: &mut ContainerBuilder) -> Result<(), RegistrationError> {
        let repository: Arc<dyn Repository> = Arc::new(ReplacementRepository);
        builder.register_instance_with(
            repository,
            BindingOptions {
                id: Some("example.repository".into()),
                ..BindingOptions::default()
            },
        )
    }
}

struct SharedSourceA;
struct SharedSourceB;

impl ComponentDefinition for SharedSourceA {
    fn source() -> DefinitionSource {
        SHARED_SOURCE
    }
    fn definition_id() -> &'static str {
        "test::discovery_tests::SharedSourceA"
    }
    fn register(builder: &mut ContainerBuilder) -> Result<(), RegistrationError> {
        DefinitionDraft::<Self>::from_instance(Self::source(), BindingOptions::default(), Arc::new(Self))?
            .register(builder)
    }
}

impl ComponentDefinition for SharedSourceB {
    fn source() -> DefinitionSource {
        SHARED_SOURCE
    }
    fn definition_id() -> &'static str {
        "test::discovery_tests::SharedSourceB"
    }
    fn register(builder: &mut ContainerBuilder) -> Result<(), RegistrationError> {
        DefinitionDraft::<Self>::from_instance(Self::source(), BindingOptions::default(), Arc::new(Self))?
            .register(builder)
    }
}

qubit_ioc::__private::submit_component!(RegistrationEntry::new(
    SharedSourceA::register,
    SHARED_SOURCE,
    "test::discovery_tests::SharedSourceA",
));
qubit_ioc::__private::submit_component!(RegistrationEntry::new(
    SharedSourceB::register,
    SHARED_SOURCE,
    "test::discovery_tests::SharedSourceB",
));

fn repository_key() -> BindingKey {
    BindingKey::of::<dyn Repository>(Some(BindingId::parse("example.repository").expect("valid ID")))
}

#[test]
fn test_discover_sorts_entries_before_registration_and_does_not_run_factories() {
    let _guard = TEST_LOCK.lock().expect("test mutex");
    CONSTRUCTION_ORDER.lock().expect("order mutex").clear();
    let builder = ContainerBuilder::new().discover().expect("discover linked definitions");
    assert!(CONSTRUCTION_ORDER.lock().expect("order mutex").is_empty());
    builder.build_all().expect("build discovered definitions");
    assert_eq!(*CONSTRUCTION_ORDER.lock().expect("order mutex"), ["first", "second"]);
}

#[test]
fn test_manual_install_plus_discovery_reports_duplicate_at_build() {
    let _guard = TEST_LOCK.lock().expect("test mutex");
    let mut builder = ContainerBuilder::new();
    builder.install::<SortedFirst>().expect("manual install");
    let builder = builder.discover().expect("discovery stages duplicates");
    assert!(matches!(builder.build_all(), Err(BuildError::DuplicateBinding { .. })));
}

#[test]
fn test_exclude_definition_skips_concrete_and_alias_and_reports_missing_dependency() {
    let _guard = TEST_LOCK.lock().expect("test mutex");
    let mut builder = ContainerBuilder::new();
    builder.exclude_definition::<MemoryRepository>();
    let builder = builder.discover().expect("discover with exclusion");
    let context = builder.build_all().expect("excluded definition has no consumer");
    assert!(
        context
            .try_get::<MemoryRepository>()
            .expect("lookup concrete")
            .is_none()
    );
    assert!(context.try_get::<dyn Repository>().expect("lookup alias").is_none());

    let mut builder = ContainerBuilder::new();
    builder.exclude_definition::<MemoryRepository>();
    builder
        .register_factory::<String, _>(
            &[Dependency::with_id::<dyn Repository>("example.repository")],
            |context| {
                let repository = context
                    .get_by_id::<dyn Repository>("example.repository")
                    .expect("declared repository");
                Ok(Arc::new(repository.label().to_owned()))
            },
        )
        .expect("stage consumer");
    assert!(matches!(
        builder.discover().expect("discover").build_all(),
        Err(BuildError::MissingDependency { .. })
    ));
}

#[test]
fn test_exclude_uses_definition_id_when_sources_are_equal() {
    let _guard = TEST_LOCK.lock().expect("test mutex");
    let mut builder = ContainerBuilder::new();
    builder.exclude_definition::<SharedSourceA>();
    let context = builder
        .discover()
        .expect("discover equal source entries")
        .build_all()
        .expect("build remaining definition");
    assert!(context.try_get::<SharedSourceA>().expect("excluded lookup").is_none());
    assert!(context.try_get::<SharedSourceB>().expect("retained lookup").is_some());
}

#[test]
fn test_replace_binding_changes_only_the_selected_interface_key() {
    let _guard = TEST_LOCK.lock().expect("test mutex");
    let mut builder = ContainerBuilder::new().discover().expect("discover source component");
    builder
        .replace_binding(repository_key(), ReplacementRepository::register)
        .expect("replace interface binding");
    let context = builder.build_all().expect("build exact replacement");
    assert!(context.get_by_id::<MemoryRepository>("example.repository").is_ok());
    assert_eq!(
        context
            .get_by_id::<dyn Repository>("example.repository")
            .expect("replacement interface")
            .label(),
        "replacement"
    );
    let (replacement_source, replaced_sources) = context
        .binding_sources(&repository_key())
        .expect("replacement provenance");
    assert!(replacement_source.item.contains("Repository"));
    assert_eq!(replaced_sources, &[MEMORY_SOURCE]);
}

#[test]
#[allow(clippy::result_large_err)]
fn test_replacing_concrete_binding_rejects_its_stale_alias_before_factory() {
    let _guard = TEST_LOCK.lock().expect("test mutex");
    let mut builder = ContainerBuilder::new().discover().expect("discover source component");
    let key = BindingKey::of::<MemoryRepository>(Some(BindingId::parse("example.repository").expect("valid id")));
    builder
        .replace_binding(key, |builder| {
            let options = BindingOptions {
                id: Some("example.repository".into()),
                ..BindingOptions::default()
            };
            let draft = DefinitionDraft::<MemoryRepository>::new_sync(
                DefinitionSource::new("test", "discovery_tests", "discovery_tests.rs", 80, 1, "replacement"),
                &[],
                options,
                |_| Ok(Arc::new(MemoryRepository)),
            )?;
            draft.register(builder)
        })
        .expect("replace concrete key");
    builder.root::<dyn Repository>();
    assert!(matches!(builder.build(), Err(BuildError::AliasTargetReplaced { .. })));
}

#[test]
fn test_definition_draft_rejects_duplicate_alias_without_partial_registration() {
    let mut builder = ContainerBuilder::new();
    let mut draft = DefinitionDraft::<MemoryRepository>::new_sync(
        DefinitionSource::new("test", "discovery_tests", "discovery_tests.rs", 40, 1, "bad draft"),
        &[],
        BindingOptions::default(),
        |_| Ok(Arc::new(MemoryRepository)),
    )
    .expect("valid concrete");
    draft
        .bind::<dyn Repository, _>(BindingOptions::default(), |value| value)
        .expect("first alias");
    draft
        .bind::<dyn Repository, _>(BindingOptions::default(), |value| value)
        .expect("duplicate alias staged in draft");
    assert!(matches!(
        draft.register(&mut builder),
        Err(RegistrationError::DuplicateDefinitionKey { .. })
    ));
    assert!(
        builder
            .build_all()
            .expect("failed draft left no bindings")
            .try_get::<MemoryRepository>()
            .expect("lookup")
            .is_none()
    );
}
