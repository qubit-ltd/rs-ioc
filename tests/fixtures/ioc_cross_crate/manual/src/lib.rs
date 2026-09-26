// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Standalone manual assembly path with no default IoC features.

use std::sync::Arc;

use qubit_ioc::ApplicationContext;
use qubit_ioc::BuildError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Dependency;
use qubit_ioc::RegistrationError;

/// Builds a shared component from an explicit instance and factory.
///
/// Registration and graph failures are kept distinct so callers can inspect
/// either stage. The factory runs only after graph validation succeeds.
pub fn build_manual() -> Result<ApplicationContext, ManualError> {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(41_u32))?;
    builder.register_factory::<u64, _>(&[Dependency::of::<u32>()], |context| {
        let value = context.get::<u32>().expect("declared dependency");
        Ok(Arc::new(u64::from(*value) + 1))
    })?;
    builder.build_all().map_err(ManualError::Build)
}

/// Failure during explicit registration or construction.
#[derive(Debug, thiserror::Error)]
pub enum ManualError {
    /// The binding declaration is invalid.
    #[error(transparent)]
    Registration(#[from] RegistrationError),
    /// The declared graph or factory failed.
    #[error(transparent)]
    Build(#[from] BuildError),
}

#[cfg(test)]
mod tests {
    use crate::build_manual;

    #[test]
    fn test_manual_assembly_without_default_features() {
        let context = build_manual().expect("build explicit graph");
        assert_eq!(*context.get::<u64>().expect("factory result"), 42);
    }
}
