// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Exercises conditional component fields at an external crate boundary.

use r#type::Component;
use r#type::bean;
type MissingType = u32;
#[cfg(all(test, feature = "extra"))]
use r#type::BuildError;
#[cfg(test)]
use r#type::ContainerBuilder;
#[cfg(all(test, feature = "extra"))]
use r#type::Dependency;

/// Includes a dependency only when the consumer enables `extra`.
#[Component]
pub struct Conditional {
    #[cfg(feature = "extra")]
    extra: std::sync::Arc<u8>,
}

/// Uses a nested conditional activation attribute.
#[Component]
pub struct NestedCondition {
    #[cfg_attr(not(feature = "extra"), cfg(any()))]
    extra: std::sync::Arc<u16>,
}

/// Contains an unavailable configuration value behind a false condition.
#[Component]
pub struct DisabledValue {
    #[cfg(any())]
    #[value("unused.port")]
    port: u16,
}

/// Contains an unsupported field type behind a false condition.
#[Component]
pub struct DisabledUnknownType {
    #[cfg(any())]
    missing: MissingType,
}

/// Activates a bean dependency only when the matching function parameter
/// exists.
#[bean(marker = ConditionalFactory)]
pub fn conditional_bean(#[cfg(feature = "extra")] value: std::sync::Arc<u8>) -> u32 {
    #[cfg(feature = "extra")]
    let _ = value;
    17
}

/// Requests a configuration value only when its parameter is enabled.
#[bean(marker = ConditionalConfigFactory)]
pub fn conditional_config_bean(
    #[cfg(feature = "value_input")]
    #[value("test.conditional")]
    value: String,
) -> u32 {
    #[cfg(feature = "value_input")]
    let _ = value;
    19
}

/// Exercises an active configuration-backed field with the consumer config
/// feature.
#[cfg(feature = "config")]
#[Component]
pub struct EnabledValue {
    #[value("test.enabled")]
    value: String,
}

/// Removes the entire component, including registration, under a false cfg.
#[Component]
#[cfg(any())]
pub struct DisabledStruct;

/// Removes the component when cfg_attr enables a false activation condition.
#[Component]
#[cfg_attr(not(feature = "extra"), cfg(any()), derive(Clone))]
pub struct ConditionalStruct;

/// Retains a non-activation derive when cfg_attr selects an active component.
#[Component]
#[cfg_attr(feature = "extra", cfg(all()), derive(Clone))]
pub struct DerivedConditionalStruct;

#[test]
fn test_struct_activation_preserves_enabled_definition_and_derives() {
    let mut builder = ContainerBuilder::new();
    builder
        .install::<DerivedConditionalStruct>()
        .expect("install active conditional struct");
    #[cfg(feature = "extra")]
    builder
        .install::<ConditionalStruct>()
        .expect("install conditionally active struct");
    let application = builder
        .build_all()
        .expect("active structs should register without dependencies");
    let context = application.context();
    let component = context
        .get::<DerivedConditionalStruct>()
        .expect("active struct should be available");
    #[cfg(feature = "extra")]
    {
        let _: DerivedConditionalStruct = component.as_ref().clone();
        context
            .get::<ConditionalStruct>()
            .expect("conditionally active struct should be available");
    }
    #[cfg(not(feature = "extra"))]
    let _ = component;
}

#[test]
fn test_bean_parameter_activation_matches_dependency_and_call_argument() {
    #[cfg(feature = "extra")]
    use std::sync::Arc;

    let mut builder = ContainerBuilder::new();
    #[cfg(feature = "extra")]
    builder.register_instance(Arc::new(23_u8)).expect("register u8");
    builder
        .install::<ConditionalFactory>()
        .expect("install conditional bean");
    let application = builder
        .build_all()
        .expect("only active parameter dependencies are requested");
    let context = application.context();
    assert_eq!(*context.get::<u32>().expect("conditional bean"), 17);
}

#[test]
#[cfg(not(feature = "extra"))]
fn test_disabled_fields_do_not_register_requests_or_require_config() {
    let mut builder = ContainerBuilder::new();
    builder.install::<Conditional>().expect("install conditional");
    builder.install::<NestedCondition>().expect("install nested condition");
    builder.install::<DisabledValue>().expect("install disabled value");
    builder.install::<DisabledUnknownType>().expect("install missing type");
    drop(builder.build_all().expect("all fields are disabled"));
}

#[cfg(feature = "extra")]
#[test]
fn test_enabled_fields_register_their_dependencies() {
    use std::sync::Arc;

    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(3_u8)).expect("register u8");
    builder.register_instance(Arc::new(5_u16)).expect("register u16");
    builder.install::<Conditional>().expect("install conditional");
    builder.install::<NestedCondition>().expect("install nested condition");
    let application = builder.build_all().expect("enabled dependencies are present");
    let context = application.context();
    assert_eq!(*context.get::<u8>().expect("u8"), 3);
}

#[cfg(feature = "extra")]
#[test]
fn test_enabled_field_reports_a_missing_dependency_before_construction() {
    let mut builder = ContainerBuilder::new();
    builder.install::<Conditional>().expect("install conditional");
    let error = builder.build_all().err().expect("u8 dependency is required");
    assert!(matches!(error.cause(), BuildError::MissingDependency { dependency, .. }
        if dependency == &Dependency::of::<u8>()));
}
