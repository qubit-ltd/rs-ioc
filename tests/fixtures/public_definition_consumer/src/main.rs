// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Manual concrete and interface registration without macro or config features.

mod api;
mod service;

use std::sync::Arc;

use qubit_ioc::BindingOptions;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Definition;
use qubit_ioc::DefinitionSource;

use crate::api::Api;
use crate::service::Service;

/// Builds one public definition and checks both bindings share its allocation.
fn verify_public_definition() {
    let value = Arc::new(Service(42));
    let definition = Definition::<Service>::builder()
        .source(DefinitionSource::new(
            "consumer",
            "consumer",
            "src/main.rs",
            1,
            1,
            "Service",
        ))
        .binding(BindingOptions::default())
        .instance(Arc::clone(&value))
        .bind::<dyn Api, _>(BindingOptions::default(), |concrete| concrete)
        .build()
        .expect("valid public definition");
    let mut builder = ContainerBuilder::new();
    builder
        .register_definition(definition)
        .expect("register public definition");
    builder.root::<dyn Api>();
    let application = builder.build().expect("build public graph");
    let context = application.context();
    let concrete = context.get::<Service>().expect("concrete binding");
    let alias = context.get::<dyn Api>().expect("interface alias");
    assert_eq!(alias.value(), 42);
    assert!(Arc::ptr_eq(&value, &concrete));
    assert_eq!(Arc::as_ptr(&concrete) as *const (), Arc::as_ptr(&alias) as *const ());
}

/// Runs the standalone consumer's public API contract.
fn main() {
    verify_public_definition();
}

