// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
// qubit-style: allow type-file-name
//! Linked definition discovery, available with the `inventory` feature.

use crate::builder::ContainerBuilder;
use crate::error::RegistrationError;
use crate::options::DefinitionSource;

/// A static registration callback, diagnostic source and stable definition ID.
///
/// The callback stages metadata; it must not construct its component.
///
/// # Examples
///
/// ```
/// use qubit_ioc::ContainerBuilder;
/// use qubit_ioc::DefinitionSource;
/// use qubit_ioc::RegistrationError;
/// use qubit_ioc::discovery::RegistrationEntry;
///
/// fn register(_: &mut ContainerBuilder) -> Result<(), RegistrationError> { Ok(()) }
/// let source = DefinitionSource::new("app", "app", "src/main.rs", 1, 1, "Service");
/// let _entry = RegistrationEntry::new(register, source, "app::Service");
/// ```
pub struct RegistrationEntry {
    /// Callback that stages metadata without constructing the component.
    register: fn(&mut ContainerBuilder) -> Result<(), RegistrationError>,
    /// Source used to sort linked entries and report registration errors.
    source: DefinitionSource,
    /// Stable identity used to exclude an entry from discovery.
    definition_id: &'static str,
}

impl RegistrationEntry {
    /// Constructs an entry from a generated or handwritten registration
    /// function.
    ///
    /// # Parameters
    ///
    /// `register` stages one definition. `source` supplies stable sorting and
    /// diagnostics. `definition_id` identifies the entry for exclusions.
    ///
    /// # Returns
    ///
    /// A static discovery entry; construction does not invoke `register`.
    pub const fn new(
        register: fn(&mut ContainerBuilder) -> Result<(), RegistrationError>,
        source: DefinitionSource,
        definition_id: &'static str,
    ) -> Self {
        Self {
            register,
            source,
            definition_id,
        }
    }
}

inventory::collect!(RegistrationEntry);

// RegistrationError retains the linked declaration's complete source.
#[allow(clippy::result_large_err)]
impl ContainerBuilder {
    /// Stages linked definitions in source order without running factories.
    ///
    /// A registration error stops discovery and returns that error. Repeating
    /// discovery stages the definitions again; active collisions fail at build.
    ///
    /// # Returns
    ///
    /// The builder with every non-excluded entry staged in source order.
    ///
    /// # Errors
    ///
    /// Returns the first [`RegistrationError`] produced by an entry callback.
    pub fn discover(mut self) -> Result<Self, RegistrationError> {
        let mut entries: Vec<_> = inventory::iter::<RegistrationEntry>.into_iter().collect();
        entries.sort_by(|left, right| {
            left.source
                .cmp(&right.source)
                .then_with(|| left.definition_id.cmp(right.definition_id))
        });
        for entry in entries {
            let excluded = self.excluded_definitions.contains(&entry.definition_id);
            if !excluded {
                (entry.register)(&mut self)?;
            }
        }
        Ok(self)
    }
}
