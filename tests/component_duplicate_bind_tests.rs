// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
#![cfg(feature = "macros")]

use qubit_ioc::Component;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::RegistrationError;

trait Repository: Send + Sync {}

#[Component(bind = dyn Repository, bind = dyn Repository)]
struct DuplicateAlias;

impl Repository for DuplicateAlias {}

#[test]
fn test_component_macro_rejects_repeated_interface_key_atomically() {
    let mut builder = ContainerBuilder::new();
    assert!(matches!(
        builder.install::<DuplicateAlias>(),
        Err(RegistrationError::DuplicateDefinitionKey { .. })
    ));
    let context = builder.build().expect("rejected definition did not stage bindings");
    assert!(
        context
            .try_get::<DuplicateAlias>()
            .expect("lookup rejected concrete")
            .is_none()
    );
}
