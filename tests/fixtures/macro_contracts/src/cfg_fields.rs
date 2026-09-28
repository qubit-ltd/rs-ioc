// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Exercises conditional component fields at an external crate boundary.

use r#type::Component;
#[cfg(all(test, feature = "extra"))]
use r#type::BuildError;
#[cfg(all(test, feature = "extra"))]
use r#type::Dependency;

#[Component]
/// Includes a dependency only when the consumer enables `extra`.
pub struct Conditional {
    #[cfg(feature = "extra")]
    extra: std::sync::Arc<u8>,
}

#[Component]
/// Uses a nested conditional activation attribute.
pub struct NestedCondition {
    #[cfg_attr(not(feature = "extra"), cfg(any()))]
    extra: std::sync::Arc<u16>,
}

#[Component]
/// Contains an unavailable configuration value behind a false condition.
pub struct DisabledValue {
    #[cfg(any())]
    #[value("unused.port")]
    port: u16,
}

#[Component]
/// Contains an unsupported field type behind a false condition.
pub struct DisabledUnknownType {
    #[cfg(any())]
    missing: std::sync::Arc<MissingType>,
}

#[cfg(feature = "config")]
#[Component]
/// Exercises an active configuration-backed field with the consumer config feature.
pub struct EnabledValue {
    #[value("test.enabled")]
    value: String,
}

#[test]
#[cfg(not(feature = "extra"))]
fn test_disabled_fields_do_not_register_requests_or_require_config() {
    let mut builder = r#type::ContainerBuilder::new();
    builder.install::<Conditional>().expect("install conditional");
    builder.install::<NestedCondition>().expect("install nested condition");
    builder.install::<DisabledValue>().expect("install disabled value");
    builder.install::<DisabledUnknownType>().expect("install missing type");
    builder.build_all().expect("all fields are disabled");
}

#[cfg(feature = "extra")]
#[test]
fn test_enabled_fields_register_their_dependencies() {
    use std::sync::Arc;

    let mut builder = r#type::ContainerBuilder::new();
    builder.register_instance(Arc::new(3_u8)).expect("register u8");
    builder.register_instance(Arc::new(5_u16)).expect("register u16");
    builder.install::<Conditional>().expect("install conditional");
    builder.install::<NestedCondition>().expect("install nested condition");
    let context = builder.build_all().expect("enabled dependencies are present");
    assert_eq!(*context.get::<u8>().expect("u8"), 3);
}

#[cfg(feature = "extra")]
#[test]
fn test_enabled_field_reports_a_missing_dependency_before_construction() {
    let mut builder = r#type::ContainerBuilder::new();
    builder.install::<Conditional>().expect("install conditional");
    let error = builder.build_all().err().expect("u8 dependency is required");
    assert!(matches!(error, BuildError::MissingDependency { dependency, .. }
        if dependency == Dependency::of::<u8>()));
}
