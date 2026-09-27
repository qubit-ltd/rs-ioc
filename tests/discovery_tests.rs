// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
#![cfg(feature = "inventory")]

use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use qubit_ioc::__private::codegen_v1::DefinitionDraft;
use qubit_ioc::__private::submit_component;
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
submit_component!(RegistrationEntry::new(
    SortedSecond::register,
    SECOND_SOURCE,
    "test::discovery_tests::SortedSecond",
));
submit_component!(RegistrationEntry::new(
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

submit_component!(RegistrationEntry::new(
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

submit_component!(RegistrationEntry::new(
    SharedSourceA::register,
    SHARED_SOURCE,
    "test::discovery_tests::SharedSourceA",
));
submit_component!(RegistrationEntry::new(
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
fn captured_replacement_uses_runtime_value() {
    let original = Arc::new(7_u64);
    let replacement = Arc::new(17_u64);
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::clone(&original)).unwrap();
    let captured = Arc::clone(&replacement);
    builder
        .replace_binding(BindingKey::of::<u64>(None), move |draft| {
            draft.register_instance(captured)
        })
        .unwrap();
    let context = builder.build_all().unwrap();
    assert!(Arc::ptr_eq(&context.get::<u64>().unwrap(), &replacement));
}

#[test]
#[allow(clippy::result_large_err)]
fn captured_replacement_error_leaves_builder_unchanged() {
    let original = Arc::new(7_u64);
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::clone(&original)).unwrap();
    let key = BindingKey::of::<u64>(None);
    let error_key = key.clone();
    let captured = Arc::new(19_u64);
    let error = builder
        .replace_binding(key, move |draft| {
            draft.register_instance(captured)?;
            Err(RegistrationError::ReplacementTargetMissing { key: error_key })
        })
        .unwrap_err();
    assert!(matches!(error, RegistrationError::ReplacementTargetMissing { .. }));
    let context = builder.build_all().unwrap();
    assert!(Arc::ptr_eq(&context.get::<u64>().unwrap(), &original));
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

#[test]
#[allow(clippy::result_large_err)]
fn test_replace_binding_rejects_missing_active_original_before_factory() {
    let factory_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let make_builder = || replacement_builder(0, Arc::clone(&factory_calls));

    let mut builder = make_builder();
    builder.register_instance(Arc::new(true)).expect("register root");
    builder.root::<bool>();
    assert_replacement_original_error(builder.build().map(|_| ()), 0);

    assert_replacement_original_error(make_builder().build_all().map(|_| ()), 0);

    let mut builder = make_builder();
    builder.register_instance(Arc::new(true)).expect("register root");
    builder.root::<bool>();
    assert_replacement_original_error(ready(builder.build_async()).map(|_| ()), 0);

    assert_replacement_original_error(ready(make_builder().build_all_async()).map(|_| ()), 0);
    assert_eq!(factory_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[test]
#[allow(clippy::result_large_err)]
fn test_replace_binding_rejects_ambiguous_originals_for_all_build_apis() {
    let factory_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let make_builder = || replacement_builder(2, Arc::clone(&factory_calls));

    let mut builder = make_builder();
    builder.register_instance(Arc::new(true)).expect("register root");
    builder.root::<bool>();
    assert_replacement_original_error(builder.build().map(|_| ()), 2);

    assert_replacement_original_error(make_builder().build_all().map(|_| ()), 2);

    let mut builder = make_builder();
    builder.register_instance(Arc::new(true)).expect("register root");
    builder.root::<bool>();
    assert_replacement_original_error(ready(builder.build_async()).map(|_| ()), 2);

    assert_replacement_original_error(ready(make_builder().build_all_async()).map(|_| ()), 2);
    assert_eq!(factory_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[allow(clippy::result_large_err)]
fn replacement_builder(originals: usize, factory_calls: Arc<std::sync::atomic::AtomicUsize>) -> ContainerBuilder {
    let mut builder = ContainerBuilder::new();
    for value in 0..originals {
        builder
            .register_instance(Arc::new(value as u64))
            .expect("register original");
    }
    let captured_calls = Arc::clone(&factory_calls);
    builder
        .replace_binding(BindingKey::of::<u64>(None), move |draft| {
            draft.register_factory::<u64, _>(&[], move |_| {
                captured_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(Arc::new(7))
            })
        })
        .expect("replacement definition declares its key");
    builder
}

fn assert_replacement_original_error(result: Result<(), BuildError>, expected_originals: usize) {
    match (result, expected_originals) {
        (Err(BuildError::ReplacementOriginalMissing { key, .. }), 0) => {
            assert_eq!(key, BindingKey::of::<u64>(None));
        }
        (Err(BuildError::ReplacementOriginalAmbiguous { key, originals, .. }), 2) => {
            assert_eq!(key, BindingKey::of::<u64>(None));
            assert_eq!(originals.len(), 2);
        }
        (result, expected) => panic!("expected replacement error for {expected} originals, got {result:?}"),
    }
}

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("build future unexpectedly suspended"),
    }
}

#[test]
#[allow(clippy::result_large_err)]
fn test_replace_binding_rejects_ambiguous_active_originals() {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(1_u64)).expect("first original");
    builder.register_instance(Arc::new(2_u64)).expect("second original");
    builder
        .replace_binding(BindingKey::of::<u64>(None), |draft| {
            draft.register_instance(Arc::new(3_u64))
        })
        .expect("replacement definition declares its key");

    let error = match builder.build_all() {
        Ok(_) => panic!("ambiguous originals must fail"),
        Err(error) => error,
    };
    let BuildError::ReplacementOriginalAmbiguous {
        key,
        originals,
        replacement: _,
    } = error
    else {
        panic!("expected replacement source ambiguity");
    };
    assert_eq!(key, BindingKey::of::<u64>(None));
    assert_eq!(originals.len(), 2);
    assert_ne!(originals[0], originals[1]);
    assert!(originals[0].line < originals[1].line);
}

#[test]
#[allow(clippy::result_large_err)]
fn test_replace_binding_counts_originals_after_profile_filtering() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_instance_with(
            Arc::new(1_u64),
            BindingOptions {
                profile: Some("prod".into()),
                ..BindingOptions::default()
            },
        )
        .expect("profiled original");
    builder
        .replace_binding(BindingKey::of::<u64>(None), |draft| {
            draft.register_instance(Arc::new(2_u64))
        })
        .expect("default-profile replacement");
    assert!(matches!(
        builder.build_all(),
        Err(BuildError::ReplacementOriginalMissing { .. })
    ));

    let mut builder = ContainerBuilder::new();
    builder
        .replace_binding(BindingKey::of::<u64>(None), |draft| {
            draft.register_instance_with(
                Arc::new(2_u64),
                BindingOptions {
                    profile: Some("prod".into()),
                    ..BindingOptions::default()
                },
            )
        })
        .expect("inactive replacement");
    assert!(builder.build_all().unwrap().get_all::<u64>().is_empty());

    let mut builder = ContainerBuilder::new();
    builder
        .register_instance_with(
            Arc::new(1_u64),
            BindingOptions {
                profile: Some("prod".into()),
                ..BindingOptions::default()
            },
        )
        .expect("profiled original");
    builder
        .replace_binding(BindingKey::of::<u64>(None), |draft| {
            draft.register_instance_with(
                Arc::new(2_u64),
                BindingOptions {
                    profile: Some("prod".into()),
                    ..BindingOptions::default()
                },
            )
        })
        .expect("profiled replacement");
    let context = builder.active_profiles(&["prod"]).unwrap().build_all().unwrap();
    assert_eq!(*context.get::<u64>().unwrap(), 2);
    let (_, replaced_sources) = context.binding_sources(&BindingKey::of::<u64>(None)).unwrap();
    assert_eq!(replaced_sources.len(), 1);
}

#[test]
#[allow(clippy::result_large_err)]
fn test_consecutive_replacements_preserve_the_full_source_chain() {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(1_u64)).expect("original");
    builder
        .replace_binding(BindingKey::of::<u64>(None), |draft| {
            draft.register_instance(Arc::new(2_u64))
        })
        .expect("first replacement");
    builder
        .replace_binding(BindingKey::of::<u64>(None), |draft| {
            draft.register_instance(Arc::new(3_u64))
        })
        .expect("second replacement");

    let context = builder.build_all().unwrap();
    assert_eq!(*context.get::<u64>().unwrap(), 3);
    let (_, replaced_sources) = context.binding_sources(&BindingKey::of::<u64>(None)).unwrap();
    assert_eq!(replaced_sources.len(), 2);
}

#[test]
#[allow(clippy::result_large_err)]
fn test_replace_binding_rejects_later_duplicate_original_as_duplicate_binding() {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(1_u64)).expect("original");
    builder
        .replace_binding(BindingKey::of::<u64>(None), |draft| {
            draft.register_instance(Arc::new(2_u64))
        })
        .expect("replacement");
    builder.register_instance(Arc::new(4_u64)).expect("later duplicate");

    assert!(matches!(
        builder.build_all(),
        Err(BuildError::DuplicateBinding { key, .. }) if key == BindingKey::of::<u64>(None)
    ));
}
