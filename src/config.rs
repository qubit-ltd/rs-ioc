// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Configuration snapshot registration and typed reads for generated factories.

use std::sync::Arc;

use qubit_config::Config;
use qubit_config::conversion::FromConfig;
use serde::de::DeserializeOwned;

use crate::builder::ContainerBuilder;
use crate::error::FactoryError;
use crate::error::RegistrationError;

// RegistrationError retains the original binding source for diagnostics.
#[allow(clippy::result_large_err)]
impl ContainerBuilder {
    /// Registers `config` as the unnamed shared configuration snapshot.
    ///
    /// The builder is returned for further registrations. An invalid
    /// registration returns [`RegistrationError`]; a second active `Config`
    /// binding with the same key is reported when the builder is built.
    #[track_caller]
    pub fn with_config(mut self, config: Config) -> Result<Self, RegistrationError> {
        self.register_instance(Arc::new(config))?;
        Ok(self)
    }
}

/// Reads a scalar at `path` using the snapshot's direct, non-interpolating
/// read.
///
/// `T` must support [`FromConfig`]. Missing values and conversion failures
/// retain their original [`qubit_config::ConfigError`] and path as the source.
pub fn get_value<T: FromConfig>(config: &Config, path: &str) -> Result<T, FactoryError> {
    config.get::<T>(path).map_err(FactoryError::new)
}

/// Reads a generated field or parameter, retaining its target for build errors.
///
/// Missing values and conversions return a factory error whose source is the
/// original Config error. This function is a macro code-generation protocol.
#[doc(hidden)]
pub fn get_value_for<T: FromConfig>(config: &Config, path: &str, target: &str) -> Result<T, FactoryError> {
    config
        .get::<T>(path)
        .map_err(|error| FactoryError::config_read(error, path, target))
}

/// Deserializes a subtree at `prefix` without interpolation.
///
/// `T` must be an owned Serde target. The default `Config::deserialize`
/// rejects unknown fields. Any configuration error keeps its root-relative
/// field path in the retained source chain.
pub fn deserialize_properties<T: DeserializeOwned>(config: &Config, prefix: &str) -> Result<T, FactoryError> {
    config.deserialize::<T>(prefix).map_err(FactoryError::new)
}

/// Deserializes a generated properties type and retains its name for errors.
///
/// The Config error stays as the direct source of the factory error. This
/// function is a macro code-generation protocol.
#[doc(hidden)]
pub fn deserialize_properties_for<T: DeserializeOwned>(
    config: &Config,
    prefix: &str,
    target: &str,
) -> Result<T, FactoryError> {
    config
        .deserialize::<T>(prefix)
        .map_err(|error| FactoryError::config_read(error, prefix, target))
}
