// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Generated component-definition registration contract.

use super::container_builder::ContainerBuilder;
use crate::error::RegistrationError;
use crate::options::DefinitionSource;

/// A declaration that can install itself into a container builder.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use qubit_ioc::ComponentDefinition;
/// use qubit_ioc::ContainerBuilder;
/// use qubit_ioc::DefinitionSource;
/// use qubit_ioc::RegistrationError;
///
/// struct Message;
/// impl ComponentDefinition for Message {
///     fn source() -> DefinitionSource {
///         DefinitionSource::new("app", "app", "src/main.rs", 1, 1, "Message")
///     }
///     fn register(builder: &mut ContainerBuilder) -> Result<(), RegistrationError> {
///         builder.register_instance(Arc::new(Message))
///     }
/// }
///
/// let mut builder = ContainerBuilder::new();
/// builder.install::<Message>()?;
/// assert!(builder.build_all()?.context().get::<Message>().is_ok());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub trait ComponentDefinition {
    /// Returns this definition's diagnostic source location.
    ///
    /// # Returns
    ///
    /// The package, module, file, and item location used in diagnostics.
    #[must_use]
    fn source() -> DefinitionSource;

    /// Registers this definition into `builder`.
    ///
    /// # Parameters
    ///
    /// `builder` receives the definition and its generated binding aliases.
    ///
    /// # Errors
    ///
    /// Returns the registration error produced when one of the definition's
    /// keys, options, or declared requests is invalid.
    // Keep the public error's full key and source fields in generated definitions.
    #[allow(clippy::result_large_err)]
    fn register(builder: &mut ContainerBuilder) -> Result<(), RegistrationError>;
}
