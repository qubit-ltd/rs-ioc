// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Verifies generated registration code against consumer-local names.

use std::sync::Arc;

use r#type::ContainerBuilder;

use crate::symbols::CollisionBean;
use crate::symbols::OrdinaryBean;

#[test]
fn test_generated_code_ignores_internal_parameter_names() {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(2_u8)).expect("register u8");
    builder.register_instance(Arc::new(3_u16)).expect("register u16");
    builder.install::<OrdinaryBean>().expect("install ordinary");
    builder.install::<CollisionBean>().expect("install collision");
    let application = builder.build_all().expect("build definitions");
    let context = application.context();
    assert_eq!(*context.get::<u32>().expect("ordinary value"), 7);
}
