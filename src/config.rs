// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Configuration snapshot registration and typed reads for generated factories.

use qubit_config::Config;
use qubit_config::conversion::FromConfig;
use serde::de::DeserializeOwned;

use crate::error::FactoryError;

/// Reads a scalar at `path` using the snapshot's direct, non-interpolating
/// read.
///
/// `T` must support [`FromConfig`]. Missing values and conversion failures
/// retain their original [`qubit_config::ConfigError`] and path as the source.
///
/// # Type Parameters
///
/// `T` is the value type to convert from the configuration entry.
///
/// # Returns
///
/// The converted value at `path`.
///
/// # Errors
///
/// Returns [`FactoryError`] when the path is absent or conversion fails.
pub fn get_value<T: FromConfig>(config: &Config, path: &str) -> Result<T, FactoryError> {
    config.get::<T>(path).map_err(FactoryError::new)
}

/// Reads a generated field or parameter, retaining its target for build errors.
///
/// Missing values and conversions return a factory error whose source is the
/// original Config error. This function is a macro code-generation protocol.
///
/// # Type Parameters
///
/// `T` is the value type to convert from the configuration entry.
///
/// # Returns
///
/// The converted value at `path`.
///
/// # Errors
///
/// Returns [`FactoryError`] with `path` and `target` context when the path is
/// absent or conversion fails.
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
///
/// # Type Parameters
///
/// `T` is the owned target type deserialized from the selected subtree.
///
/// # Returns
///
/// The deserialized value for the subtree at `prefix`.
///
/// # Errors
///
/// Returns [`FactoryError`] when the subtree is absent or cannot be
/// deserialized as `T`.
pub fn deserialize_properties<T: DeserializeOwned>(config: &Config, prefix: &str) -> Result<T, FactoryError> {
    config.deserialize::<T>(prefix).map_err(FactoryError::new)
}

/// Deserializes a generated properties type and retains its name for errors.
///
/// The Config error stays as the direct source of the factory error. This
/// function is a macro code-generation protocol.
///
/// # Type Parameters
///
/// `T` is the owned properties type deserialized from the selected subtree.
///
/// # Returns
///
/// The deserialized properties value for `prefix`.
///
/// # Errors
///
/// Returns [`FactoryError`] with `prefix` and `target` context when
/// deserialization fails.
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
