// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Factory failures and configuration read context.

use std::error::Error;

use thiserror::Error;

/// A concrete wrapper retaining the original user factory error as its source.
///
/// # Examples
///
/// ```
/// use qubit_ioc::FactoryError;
///
/// let error = FactoryError::new(std::io::Error::other("connection refused"));
/// assert!(std::error::Error::source(&error).is_some());
/// ```
#[derive(Debug, Error)]
#[error("factory failed: {source}")]
pub struct FactoryError {
    /// Original user or configuration error retained as the source chain.
    #[source]
    source: Box<dyn Error + Send + Sync + 'static>,
    /// Extra target information for generated configuration reads.
    config_read: Option<ConfigReadContext>,
}

/// Path and destination retained for a generated configuration read.
#[derive(Debug)]
struct ConfigReadContext {
    /// Configuration key or subtree prefix that failed to read.
    path_key: String,
    /// Generated field, parameter, or properties type being built.
    target: String,
}

impl FactoryError {
    /// Wraps a user error without discarding its source chain.
    pub fn new<E: Error + Send + Sync + 'static>(source: E) -> Self {
        Self {
            source: Box::new(source),
            config_read: None,
        }
    }

    /// Marks a generated configuration failure while keeping `source` intact.
    ///
    /// The path is the Config lookup key; `target` names the field, parameter,
    /// or properties type being constructed. Construction uses both to report
    /// [`BuildError::ConfigReadFailed`].
    #[cfg(feature = "config")]
    pub(crate) fn config_read<E: Error + Send + Sync + 'static>(source: E, path_key: &str, target: &str) -> Self {
        Self {
            source: Box::new(source),
            config_read: Some(ConfigReadContext {
                path_key: path_key.to_owned(),
                target: target.to_owned(),
            }),
        }
    }

    /// Returns the generated read's path and destination, when one failed.
    ///
    /// `None` indicates an ordinary factory error; `Some` borrows the stored
    /// configuration key and target name.
    pub(crate) fn config_read_context(&self) -> Option<(&str, &str)> {
        self.config_read
            .as_ref()
            .map(|context| (context.path_key.as_str(), context.target.as_str()))
    }
}
