// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::error::Error;
use std::sync::Arc;

use qubit_config::Config;
use qubit_config::ConfigError;
use qubit_ioc::BuildError;
use qubit_ioc_fixture_app::AppService;
use qubit_ioc_fixture_app::DiskRepository;
use qubit_ioc_fixture_app::Greeting;
use qubit_ioc_fixture_app::MemoryRepository;
use qubit_ioc_fixture_app::PreviewMarker;
use qubit_ioc_fixture_app::Repository;
use qubit_ioc_fixture_app::discover;

/// Creates the configuration snapshot used by linked providers.
fn fixture_config() -> Config {
    let mut config = Config::new();
    config.set("fixture.label", "demo").expect("set fixture label");
    config
}

#[test]
fn test_linked_crates_discover_aliases_and_share_component_identity() {
    let context = discover(fixture_config(), &[])
        .expect("discover linked provider crate")
        .build()
        .expect("build cross-crate graph");
    let service = context.get::<AppService>().expect("service from linked provider");
    let memory = context
        .get_by_id::<MemoryRepository>("fixture.repo.memory")
        .expect("concrete component");
    let primary = context.get::<dyn Repository>().expect("primary trait alias");
    let disk = context
        .get_by_id::<DiskRepository>("fixture.repo.disk")
        .expect("secondary concrete component");
    assert_eq!(primary.find(7), "memory:7");
    assert_eq!(service.disk.find(7), "disk:7");
    assert!(Arc::ptr_eq(&service.primary, &primary));
    assert_eq!(Arc::as_ptr(&memory) as *const (), Arc::as_ptr(&primary) as *const ());
    assert_eq!(Arc::as_ptr(&disk) as *const (), Arc::as_ptr(&service.disk) as *const ());
    assert_eq!(
        context.get::<Greeting>().expect("bean in provider crate").0,
        "demo:memory:7"
    );
    assert!(
        context
            .try_get::<PreviewMarker>()
            .expect("inactive profile lookup")
            .is_none()
    );
}

#[test]
fn test_linked_crates_respect_explicit_profiles() {
    let context = discover(fixture_config(), &["default", "preview"])
        .expect("discover with both profiles")
        .build()
        .expect("build both profiles");
    assert!(context.get::<PreviewMarker>().is_ok());
    assert!(context.get::<AppService>().is_ok());
}

#[test]
fn test_linked_properties_keep_original_config_error() {
    let error = discover(Config::new(), &[])
        .expect("discover linked definitions")
        .build()
        .err()
        .expect("missing settings must fail");
    assert!(matches!(&error, BuildError::ConfigReadFailed { path_key, target, .. }
        if path_key == "fixture" && target == "Settings"));
    let source = error.source().expect("factory error").source().expect("config error");
    assert!(source.is::<ConfigError>());
}
