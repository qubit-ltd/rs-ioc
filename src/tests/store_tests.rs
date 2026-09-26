// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Verifies erased values preserve wide pointers and aliases share one
//! allocation.

use std::sync::Arc;

use crate::binding::PendingBinding;
use crate::binding::PendingDefinition;
use crate::error::RegistrationError;
use crate::key::BindingId;
use crate::key::BindingKey;
use crate::options::DefinitionSource;
use crate::store::InstanceStore;

trait Greeter: Send + Sync {
    fn greet(&self) -> &'static str;
}

trait Counter: Send + Sync {
    fn count(&self) -> usize;
}

struct Greeting;

impl Greeter for Greeting {
    fn greet(&self) -> &'static str {
        "hello"
    }
}

impl Counter for Greeting {
    fn count(&self) -> usize {
        1
    }
}

const SOURCE: DefinitionSource = DefinitionSource::new("qubit-ioc", "store_tests", "store_tests.rs", 1, 1, "Greeting");

/// Confirms a trait-object Arc retains its metadata through type erasure.
#[test]
fn test_store_wide_pointer_round_trip() {
    let mut store = InstanceStore::default();
    let key = BindingKey::of::<dyn Greeter>(None);
    let original: Arc<dyn Greeter> = Arc::new(Greeting);

    store.insert(key.clone(), Arc::clone(&original));
    let returned = store
        .get::<dyn Greeter>(&key)
        .expect("trait-object binding should exist");

    assert!(Arc::ptr_eq(&original, &returned));
    assert_eq!(returned.greet(), "hello");
}

/// Confirms two trait projections share the concrete component's allocation.
#[test]
fn test_store_aliases_share_concrete_allocation() {
    let concrete_key = BindingKey::of::<Greeting>(None);
    let greeter_key = BindingKey::of::<dyn Greeter>(None);
    let counter_key = BindingKey::of::<dyn Counter>(None);
    let concrete = Arc::new(Greeting);
    let mut store = InstanceStore::default();
    store.insert(concrete_key.clone(), Arc::clone(&concrete));

    let greeter = PendingBinding::alias::<Greeting, dyn Greeter, _>(
        greeter_key.clone(),
        concrete_key.clone(),
        false,
        0,
        |value| value,
    );
    let counter = PendingBinding::alias::<Greeting, dyn Counter, _>(
        counter_key.clone(),
        concrete_key.clone(),
        false,
        0,
        |value| value,
    );
    let concrete_erased = store.get_erased(&concrete_key).expect("concrete binding should exist");
    let greeter_value = greeter
        .project_alias(concrete_erased)
        .expect("greeter alias should project from concrete value");
    let counter_value = counter
        .project_alias(concrete_erased)
        .expect("counter alias should project from concrete value");
    store.insert_erased(greeter_key.clone(), greeter_value);
    store.insert_erased(counter_key.clone(), counter_value);

    let greet = store
        .get::<dyn Greeter>(&greeter_key)
        .expect("greeter alias should exist");
    let count = store
        .get::<dyn Counter>(&counter_key)
        .expect("counter alias should exist");
    assert!(Arc::ptr_eq(
        &concrete,
        &store.get::<Greeting>(&concrete_key).expect("concrete")
    ));
    assert_eq!(Arc::as_ptr(&concrete) as *const (), Arc::as_ptr(&greet) as *const ());
    assert_eq!(Arc::as_ptr(&concrete) as *const (), Arc::as_ptr(&count) as *const ());
    assert_eq!(greet.greet(), "hello");
    assert_eq!(count.count(), 1);
}

/// Rejects duplicate keys within a definition before it can be staged.
#[test]
fn test_definition_duplicate_alias_key_rejected() {
    let concrete_key = BindingKey::of::<Greeting>(None);
    let alias_id = BindingId::parse("example.greeter").expect("valid ID");
    let greeter_key = BindingKey::of::<dyn Greeter>(Some(alias_id));
    let mut definition = PendingDefinition::new(
        SOURCE,
        None,
        Vec::new(),
        PendingBinding::instance(concrete_key.clone(), Arc::new(Greeting), false, 0),
    );
    definition.add_alias(PendingBinding::alias::<Greeting, dyn Greeter, _>(
        greeter_key.clone(),
        concrete_key.clone(),
        false,
        0,
        |value| value,
    ));
    definition.add_alias(PendingBinding::alias::<Greeting, dyn Greeter, _>(
        greeter_key.clone(),
        concrete_key,
        false,
        0,
        |value| value,
    ));

    let mut staged = Vec::new();
    let error = definition
        .stage_into(&mut staged)
        .expect_err("duplicate alias must be rejected");
    assert!(
        matches!(error, RegistrationError::DuplicateDefinitionKey { key, definition }
        if key == greeter_key && definition == SOURCE)
    );
    assert!(
        staged.is_empty(),
        "an invalid definition must not partially enter the staging area"
    );
}

/// Rejects an alias in the concrete binding position.
#[test]
fn test_definition_first_binding_must_be_concrete() {
    let concrete_key = BindingKey::of::<Greeting>(None);
    let alias_key = BindingKey::of::<dyn Greeter>(None);
    let definition = PendingDefinition::new(
        SOURCE,
        None,
        Vec::new(),
        PendingBinding::alias::<Greeting, dyn Greeter, _>(alias_key.clone(), concrete_key, false, 0, |value| value),
    );
    let mut staged = Vec::new();

    let error = definition
        .stage_into(&mut staged)
        .expect_err("first binding cannot be an alias");
    assert!(
        matches!(error, RegistrationError::InvalidConcreteBinding { key, definition }
        if key == alias_key && definition == SOURCE)
    );
    assert!(staged.is_empty());
}

/// Rejects an empty internal definition before it can enter the staging area.
#[test]
fn test_definition_empty_binding_list_rejected() {
    let mut definition = PendingDefinition::new(
        SOURCE,
        None,
        Vec::new(),
        PendingBinding::instance(BindingKey::of::<Greeting>(None), Arc::new(Greeting), false, 0),
    );
    definition.bindings.clear();
    let mut staged = Vec::new();

    let error = definition
        .stage_into(&mut staged)
        .expect_err("empty definition must be rejected");
    assert!(matches!(error, RegistrationError::EmptyDefinition { definition } if definition == SOURCE));
    assert!(staged.is_empty());
}

/// Rejects a second concrete binding rather than silently staging two
/// factories.
#[test]
fn test_definition_following_binding_must_be_alias() {
    let concrete_key = BindingKey::of::<Greeting>(None);
    let second_key = BindingKey::of::<dyn Greeter>(None);
    let mut definition = PendingDefinition::new(
        SOURCE,
        None,
        Vec::new(),
        PendingBinding::instance(concrete_key, Arc::new(Greeting), false, 0),
    );
    let second: Arc<dyn Greeter> = Arc::new(Greeting);
    definition.add_alias(PendingBinding::instance(second_key.clone(), second, false, 0));
    let mut staged = Vec::new();

    let error = definition
        .stage_into(&mut staged)
        .expect_err("following binding must be an alias");
    assert!(
        matches!(error, RegistrationError::InvalidAliasBinding { key, definition }
        if key == second_key && definition == SOURCE)
    );
    assert!(staged.is_empty());
}

/// Rejects an alias that projects from any key other than this definition's
/// concrete key.
#[test]
fn test_definition_alias_target_must_match_concrete_key() {
    let concrete_key = BindingKey::of::<Greeting>(None);
    let foreign_id = BindingId::parse("other").expect("valid ID");
    let foreign_key = BindingKey::of::<Greeting>(Some(foreign_id));
    let alias_key = BindingKey::of::<dyn Greeter>(None);
    let mut definition = PendingDefinition::new(
        SOURCE,
        None,
        Vec::new(),
        PendingBinding::instance(concrete_key.clone(), Arc::new(Greeting), false, 0),
    );
    definition.add_alias(PendingBinding::alias::<Greeting, dyn Greeter, _>(
        alias_key.clone(),
        foreign_key.clone(),
        false,
        0,
        |value| value,
    ));
    let mut staged = Vec::new();

    let error = definition
        .stage_into(&mut staged)
        .expect_err("alias target must be concrete key");
    assert!(matches!(error, RegistrationError::InvalidAliasTarget {
        alias,
        target,
        expected,
        definition,
    } if alias == alias_key && target == foreign_key && expected == concrete_key && definition == SOURCE));
    assert!(staged.is_empty());
}
