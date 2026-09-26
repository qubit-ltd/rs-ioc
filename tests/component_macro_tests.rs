// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
#![cfg(feature = "macros")]

use std::sync::Arc;

#[cfg(feature = "config")]
use qubit_config::Config;
#[cfg(feature = "inventory")]
use qubit_ioc::BuildError;
#[cfg(feature = "config")]
use qubit_ioc::BuildError as ConfigBuildError;
use qubit_ioc::Component;
#[cfg(feature = "inventory")]
use qubit_ioc::ComponentDefinition;
use qubit_ioc::ContainerBuilder;
#[cfg(feature = "config")]
use qubit_ioc::Dependency;
use qubit_ioc::Repository;
use qubit_ioc::Service;

trait Greeting: Send + Sync {
    fn greeting(&self) -> &'static str;
}

trait Named: Send + Sync {
    fn name(&self) -> &'static str;
}

#[Component(id = "sample.alpha", bind = dyn Greeting, bind = dyn Named, primary, order = -5)]
struct Alpha;

impl Alpha {
    const __IOC_SOURCE: u8 = 1;
    const __IOC_ID: u8 = 2;
}

impl Greeting for Alpha {
    fn greeting(&self) -> &'static str {
        "alpha"
    }
}

impl Named for Alpha {
    fn name(&self) -> &'static str {
        "Alpha"
    }
}

#[Repository(id = "sample.beta", bind = dyn Greeting, order = 3)]
struct Beta;

impl Greeting for Beta {
    fn greeting(&self) -> &'static str {
        "beta"
    }
}

#[Service]
struct Consumer {
    #[inject(id = "sample.beta")]
    selected: Arc<dyn Greeting>,
    primary: Arc<dyn Greeting>,
    optional: Option<Arc<Alpha>>,
    all: Vec<Arc<dyn Greeting>>,
}

#[Component]
struct Missing;

#[Service]
struct OptionalConsumer {
    absent: Option<Arc<Missing>>,
    all_absent: Vec<Arc<Missing>>,
}

#[Component]
struct QualifiedDependency;

#[Service]
struct QualifiedConsumer {
    required: std::sync::Arc<QualifiedDependency>,
    optional: ::std::option::Option<::std::sync::Arc<QualifiedDependency>>,
    all: ::std::vec::Vec<std::sync::Arc<QualifiedDependency>>,
}

#[Component]
struct PrivateFields {
    dependency: Arc<Alpha>,
}

#[Service]
struct DuplicateConsumer {
    first: Arc<Alpha>,
    second: Arc<Alpha>,
}

#[cfg(feature = "config")]
#[Service(profile = "values")]
struct ValueConsumer {
    #[value("sample.answer")]
    answer: i32,
}

#[cfg(feature = "config")]
#[Service(profile = "interleaved")]
#[allow(dead_code)]
struct InterleavedConsumer {
    #[value("sample.answer")]
    first: i32,
    #[inject(id = "missing.alpha")]
    second: Arc<Alpha>,
    #[value("sample.other")]
    third: i32,
}

/// Installs the dependency set used by the consumer tests.
fn install_consumer_graph() -> ContainerBuilder {
    let mut builder = ContainerBuilder::new();
    builder.install::<Alpha>().expect("install alpha");
    builder.install::<Beta>().expect("install beta");
    builder.install::<Consumer>().expect("install consumer");
    builder
}

#[test]
fn test_component_macro_builds_private_fields_and_selects_interface_aliases() {
    assert_eq!((Alpha::__IOC_SOURCE, Alpha::__IOC_ID), (1, 2));
    let context = install_consumer_graph().build_all().expect("build consumer graph");
    let consumer = context.get::<Consumer>().expect("get consumer");
    assert_eq!(consumer.selected.greeting(), "beta");
    assert_eq!(consumer.primary.greeting(), "alpha");
    assert!(consumer.optional.is_some());
    assert_eq!(
        consumer.all.iter().map(|value| value.greeting()).collect::<Vec<_>>(),
        ["alpha", "beta"]
    );

    let alpha = context.get_by_id::<Alpha>("sample.alpha").expect("concrete alpha");
    let greeting = context
        .get_by_id::<dyn Greeting>("sample.alpha")
        .expect("greeting alias");
    let named = context.get_by_id::<dyn Named>("sample.alpha").expect("named alias");
    assert_eq!(named.name(), "Alpha");
    assert_eq!(Arc::as_ptr(&alpha) as *const (), Arc::as_ptr(&greeting) as *const ());
    assert_eq!(Arc::as_ptr(&alpha) as *const (), Arc::as_ptr(&named) as *const ());
}

#[test]
fn test_component_macro_handles_optional_and_empty_collection() {
    let mut builder = ContainerBuilder::new();
    builder
        .install::<OptionalConsumer>()
        .expect("install optional consumer");
    let context = builder.build_all().expect("optional dependency can be absent");
    let consumer = context.get::<OptionalConsumer>().expect("get optional consumer");
    assert!(consumer.absent.is_none());
    assert!(consumer.all_absent.is_empty());
}

#[test]
fn test_component_macro_accepts_standard_qualified_wrapper_paths() {
    let mut builder = ContainerBuilder::new();
    builder.install::<QualifiedDependency>().expect("install dependency");
    builder.install::<QualifiedConsumer>().expect("install consumer");
    builder.root::<QualifiedConsumer>();
    let context = builder.build().expect("build qualified consumer");
    let consumer = context.get::<QualifiedConsumer>().expect("get consumer");
    assert!(Arc::ptr_eq(&consumer.required, consumer.optional.as_ref().unwrap()));
    assert_eq!(consumer.all.len(), 1);
    assert!(Arc::ptr_eq(&consumer.required, &consumer.all[0]));
}

#[test]
fn test_component_macro_preserves_unit_struct_and_private_field_construction() {
    let mut builder = ContainerBuilder::new();
    builder.install::<Alpha>().expect("install alpha");
    builder.install::<PrivateFields>().expect("install private fields");
    let context = builder.build_all().expect("build private fields graph");
    let private = context.get::<PrivateFields>().expect("get private fields");
    assert!(Arc::ptr_eq(
        &private.dependency,
        &context.get_by_id::<Alpha>("sample.alpha").expect("get alpha")
    ));
}

#[test]
fn test_component_macro_reuses_one_declared_dependency_for_two_fields() {
    let mut builder = ContainerBuilder::new();
    builder.install::<Alpha>().expect("install alpha");
    builder
        .install::<DuplicateConsumer>()
        .expect("install duplicate consumer");
    let context = builder.build_all().expect("build consumer with duplicate field type");
    let consumer = context.get::<DuplicateConsumer>().expect("get duplicate consumer");
    assert!(Arc::ptr_eq(&consumer.first, &consumer.second));
}

#[cfg(feature = "inventory")]
#[test]
fn test_component_macro_discovery_and_manual_install_share_registration() {
    let source = <Alpha as ComponentDefinition>::source();
    assert_eq!(source.item, "Alpha");
    assert_eq!(source.package, env!("CARGO_PKG_NAME"));
    assert_eq!(source.module_path, module_path!());

    let mut builder = ContainerBuilder::new();
    builder.exclude_definition::<Missing>();
    let context = builder
        .discover()
        .expect("discover component definitions")
        .build_all()
        .expect("build discovered components");
    assert!(
        context
            .try_get::<Missing>()
            .expect("excluded component lookup")
            .is_none()
    );
    assert_eq!(
        context
            .get::<Consumer>()
            .expect("discovered consumer")
            .selected
            .greeting(),
        "beta"
    );

    let mut builder = install_consumer_graph();
    builder.install::<Alpha>().expect("staging duplicate is deferred");
    assert!(matches!(builder.build_all(), Err(BuildError::DuplicateBinding { .. })));
}

#[cfg(feature = "config")]
#[test]
fn test_component_macro_reads_value_from_config_snapshot() {
    let mut config = Config::new();
    config.set("sample.answer", 42).expect("set configured answer");
    let mut builder = ContainerBuilder::new()
        .with_config(config)
        .expect("stage config snapshot")
        .active_profiles(&["values"])
        .expect("valid profile");
    builder.install::<ValueConsumer>().expect("install value consumer");
    let context = builder.build_all().expect("build configured component");
    assert_eq!(context.get::<ValueConsumer>().expect("get value consumer").answer, 42);
}

#[cfg(feature = "config")]
#[test]
fn test_component_macro_interleaves_value_dependency_in_field_order() {
    let mut builder = ContainerBuilder::new()
        .active_profiles(&["interleaved"])
        .expect("valid profile");
    builder
        .install::<InterleavedConsumer>()
        .expect("install interleaved consumer");
    let error = match builder.build_all() {
        Ok(_) => panic!("missing Config should be reported first"),
        Err(error) => error,
    };
    assert!(matches!(error, ConfigBuildError::MissingDependency { dependency, .. }
        if dependency == Dependency::of::<Config>()));
}

#[test]
fn test_component_macro_reports_unsupported_declarations() {
    trybuild::TestCases::new().compile_fail("tests/ui/component/*.rs");
}
