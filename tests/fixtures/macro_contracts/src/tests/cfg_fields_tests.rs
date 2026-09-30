// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Verifies how conditional activation attributes change registration.

#[cfg(feature = "extra")]
use std::sync::Arc;

#[cfg(feature = "extra")]
use r#type::BuildError;
use r#type::ContainerBuilder;
#[cfg(feature = "extra")]
use r#type::Dependency;

use crate::cfg_fields;

#[test]
fn test_struct_activation_preserves_enabled_definition_and_derives() {
    let mut builder = ContainerBuilder::new();
    builder
        .install::<cfg_fields::DerivedConditionalStruct>()
        .expect("install active conditional struct");
    #[cfg(feature = "extra")]
    builder
        .install::<cfg_fields::ConditionalStruct>()
        .expect("install conditionally active struct");
    let application = builder
        .build_all()
        .expect("active structs should register without dependencies");
    let context = application.context();
    let component = context
        .get::<cfg_fields::DerivedConditionalStruct>()
        .expect("active struct should be available");
    #[cfg(feature = "extra")]
    {
        let _: cfg_fields::DerivedConditionalStruct = component.as_ref().clone();
        context
            .get::<cfg_fields::ConditionalStruct>()
            .expect("conditionally active struct should be available");
    }
    #[cfg(not(feature = "extra"))]
    let _ = component;
}

#[test]
fn test_bean_parameter_activation_matches_dependency_and_call_argument() {
    let mut builder = ContainerBuilder::new();
    #[cfg(feature = "extra")]
    builder.register_instance(Arc::new(23_u8)).expect("register u8");
    builder
        .install::<cfg_fields::ConditionalFactory>()
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
    builder
        .install::<cfg_fields::Conditional>()
        .expect("install conditional");
    builder
        .install::<cfg_fields::NestedCondition>()
        .expect("install nested condition");
    builder
        .install::<cfg_fields::DisabledValue>()
        .expect("install disabled value");
    builder
        .install::<cfg_fields::DisabledUnknownType>()
        .expect("install missing type");
    drop(builder.build_all().expect("all fields are disabled"));
}

#[cfg(feature = "extra")]
#[test]
fn test_enabled_fields_register_their_dependencies() {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(3_u8)).expect("register u8");
    builder.register_instance(Arc::new(5_u16)).expect("register u16");
    builder
        .install::<cfg_fields::Conditional>()
        .expect("install conditional");
    builder
        .install::<cfg_fields::NestedCondition>()
        .expect("install nested condition");
    let application = builder.build_all().expect("enabled dependencies are present");
    let context = application.context();
    assert_eq!(*context.get::<u8>().expect("u8"), 3);
}

#[cfg(feature = "extra")]
#[test]
fn test_enabled_field_reports_a_missing_dependency_before_construction() {
    let mut builder = ContainerBuilder::new();
    builder
        .install::<cfg_fields::Conditional>()
        .expect("install conditional");
    let error = builder.build_all().err().expect("u8 dependency is required");
    assert!(matches!(error.cause(), BuildError::MissingDependency { dependency, .. }
        if dependency == &Dependency::of::<u8>()));
}
