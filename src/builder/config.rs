// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Configuration feature methods for the container builder.

use std::sync::Arc;

use qubit_config::Config;

use crate::builder::ContainerBuilder;
use crate::error::RegistrationError;

// RegistrationError retains the original binding source for diagnostics.
#[allow(clippy::result_large_err)]
impl ContainerBuilder {
    /// Registers `config` as the unnamed shared configuration snapshot.
    ///
    /// The builder is returned for further registrations. An invalid
    /// registration returns [`RegistrationError`]; a second active `Config`
    /// binding with the same key is reported when the builder is built.
    ///
    /// # Returns
    ///
    /// The builder containing the registered shared configuration snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`RegistrationError`] if staging the snapshot fails.
    #[track_caller]
    pub fn with_config(mut self, config: Config) -> Result<Self, RegistrationError> {
        self.register_instance(Arc::new(config))?;
        Ok(self)
    }
}
