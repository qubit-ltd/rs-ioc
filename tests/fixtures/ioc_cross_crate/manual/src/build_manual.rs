// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Explicit registration and construction for the manual fixture.

use std::sync::Arc;

use qubit_ioc::Application;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Dependency;

use crate::ManualError;

/// Builds a shared component from an instance and a factory.
///
/// The factory runs only after graph validation succeeds. Registration and
/// build failures remain distinguishable through [`ManualError`].
pub fn build_manual() -> Result<Application, ManualError> {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(41_u32))?;
    builder.register_factory::<u64, _>(&[Dependency::of::<u32>()], |context| {
        let value = context.get::<u32>().expect("declared dependency");
        Ok(Arc::new(u64::from(*value) + 1))
    })?;
    builder.build_all().map_err(ManualError::Build)
}
