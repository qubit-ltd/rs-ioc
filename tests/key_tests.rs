// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use std::any::TypeId;
use std::error::Error;
use std::fmt;

use qubit_ioc::BindingId;
use qubit_ioc::BindingKey;
use qubit_ioc::BindingOptions;
use qubit_ioc::DefinitionSource;
use qubit_ioc::Dependency;
use qubit_ioc::DependencyCardinality;
use qubit_ioc::FactoryError;
use qubit_ioc::RegistrationError;

#[test]
fn test_binding_id_accepts_segmented_ascii_names() {
    for value in ["Cache2", "example.cache.Local_2", "a_B.C3"] {
        let id = BindingId::parse(value).expect("valid binding ID");
        assert_eq!(id.as_str(), value);
        assert_eq!(id.to_string(), value);
    }
}

#[test]
fn test_binding_id_rejects_invalid_segments() {
    for value in ["", ".x", "x..y", "x.", "1x.y", "x-y", "x/名", "a b", "x.2y"] {
        let error = BindingId::parse(value).expect_err("invalid binding ID");
        assert_eq!(error.value(), value);
    }
}

#[test]
fn test_binding_id_preserves_case() {
    assert_ne!(
        BindingId::parse("Cache").expect("valid ID"),
        BindingId::parse("cache").expect("valid ID")
    );
}

#[test]
fn test_binding_key_distinguishes_type_and_missing_id() {
    let unnamed = BindingKey::of::<u32>(None);
    let named = BindingKey::of::<u32>(Some(BindingId::parse("cache").expect("valid ID")));
    let other_type = BindingKey::of::<u64>(Some(BindingId::parse("cache").expect("valid ID")));
    assert_ne!(unnamed, named);
    assert_ne!(named, other_type);
    assert_eq!(named.type_id(), TypeId::of::<u32>());
    assert_eq!(named.id().expect("named binding").as_str(), "cache");
}

#[test]
fn test_binding_options_keep_raw_id_for_registration_validation() {
    let options = BindingOptions {
        id: Some("invalid-id".to_owned()),
        primary: true,
        order: -2,
        profile: Some("development".to_owned()),
    };
    assert_eq!(options.id.as_deref(), Some("invalid-id"));
    assert_eq!(BindingOptions::default().id, None);
}

#[test]
fn test_dependency_constructors_preserve_cardinality_and_id() {
    let required = Dependency::of::<u32>();
    let named = Dependency::with_id::<u32>("cache");
    let optional = Dependency::optional::<u32>();
    let optional_named = Dependency::optional_with_id::<u32>("fallback");
    let all = Dependency::all::<u32>();

    for dependency in [&required, &named, &optional, &optional_named, &all] {
        assert_eq!(dependency.type_id(), TypeId::of::<u32>());
        assert_eq!(dependency.type_name(), std::any::type_name::<u32>());
    }
    assert_eq!(required.cardinality(), DependencyCardinality::Required);
    assert_eq!(required.id(), None);
    assert_eq!(named.cardinality(), DependencyCardinality::Required);
    assert_eq!(named.id(), Some("cache"));
    assert_eq!(optional.cardinality(), DependencyCardinality::Optional);
    assert_eq!(optional.id(), None);
    assert_eq!(optional_named.cardinality(), DependencyCardinality::Optional);
    assert_eq!(optional_named.id(), Some("fallback"));
    assert_eq!(all.cardinality(), DependencyCardinality::All);
    assert_eq!(all.id(), None);
}

#[derive(Debug)]
struct ExampleFactoryFailure;

impl fmt::Display for ExampleFactoryFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("factory failed")
    }
}

impl Error for ExampleFactoryFailure {}

#[test]
fn test_factory_error_preserves_source() {
    let error = FactoryError::new(ExampleFactoryFailure);
    assert_eq!(error.source().expect("source retained").to_string(), "factory failed");
}

#[test]
fn test_registration_error_preserves_duplicate_key_within_definition() {
    let key = BindingKey::of::<u32>(Some(BindingId::parse("cache").expect("valid ID")));
    let definition = DefinitionSource::new("example", "example::module", "src/lib.rs", 12, 4, "Cache");
    let error = RegistrationError::DuplicateDefinitionKey {
        key: key.clone(),
        definition,
    };

    match error {
        RegistrationError::DuplicateDefinitionKey {
            key: conflicting_key,
            definition: conflicting_definition,
        } => {
            assert_eq!(conflicting_key, key);
            assert_eq!(conflicting_definition, definition);
        }
        other => panic!("expected a duplicate definition key error, got {other:?}"),
    }
}
