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
/// # Parameters
///
/// * `config` - Snapshot that supplies the value.
/// * `path` - Exact configuration key to read without interpolation.
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
/// # Parameters
///
/// * `config` - Snapshot that supplies the value.
/// * `path` - Exact configuration key to read without interpolation.
/// * `target` - Generated field or parameter named in build diagnostics.
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
/// `T` must be an owned Serde target. Unknown-field rejection depends on what
/// the target deserializer observes: ordinary derived structs generally reject
/// unknown fields, while `flatten`, untagged variants, and custom
/// `Deserialize` implementations can consume or discard fields without
/// reporting them as unknown. For a closed schema, use
/// `#[serde(deny_unknown_fields)]` on each payload struct, including structs
/// used by untagged variants. Serde does not support combining that attribute
/// with `flatten`.
///
/// Unknown-field failures may be reported as redacted deserialization errors;
/// callers must not rely on an `UnknownProperties` variant or a complete list
/// of unknown paths. The original [`qubit_config::ConfigError`] remains in the
/// factory error's source chain, but a concrete field path is not guaranteed.
///
/// # Type Parameters
///
/// `T` is the owned target type deserialized from the selected subtree.
///
/// # Parameters
///
/// * `config` - Snapshot containing the subtree.
/// * `prefix` - Root-relative path of the subtree to deserialize.
///
/// # Returns
///
/// The deserialized value for the subtree at `prefix`.
///
/// # Errors
///
/// Returns [`FactoryError`] when the subtree is absent or cannot be
/// deserialized as `T`. Unknown-field behavior and diagnostic path guarantees
/// follow the target's Serde deserializer as described above.
pub fn deserialize_properties<T: DeserializeOwned>(config: &Config, prefix: &str) -> Result<T, FactoryError> {
    config.deserialize::<T>(prefix).map_err(FactoryError::new)
}

/// Deserializes a generated properties type and retains its name for errors.
///
/// Unknown-field rejection depends on what the target deserializer observes:
/// ordinary derived structs generally reject unknown fields, while `flatten`,
/// untagged variants, and custom `Deserialize` implementations can consume or
/// discard them. Closed schemas should use `#[serde(deny_unknown_fields)]` on
/// each payload struct, including untagged variant payloads; Serde does not
/// support combining that attribute with `flatten`.
///
/// Unknown-field failures may be redacted and need not identify every unknown
/// path or use a particular error variant. The original Config error remains
/// in the factory error's source chain, but a concrete field path is not
/// guaranteed. This function is a macro code-generation protocol.
///
/// # Type Parameters
///
/// `T` is the owned properties type deserialized from the selected subtree.
///
/// # Parameters
///
/// * `config` - Snapshot containing the subtree.
/// * `prefix` - Root-relative path of the subtree to deserialize.
/// * `target` - Generated properties type named in build diagnostics.
///
/// # Returns
///
/// The deserialized properties value for `prefix`.
///
/// # Errors
///
/// Returns [`FactoryError`] with `prefix` and `target` context when
/// deserialization fails. Unknown-field behavior and diagnostic path
/// guarantees follow the target's Serde deserializer as described above.
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
