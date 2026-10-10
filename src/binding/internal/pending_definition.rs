// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Complete concrete bindings and interface aliases staged atomically.

use std::collections::HashSet;

use crate::binding::PendingBinding;
use crate::binding::PendingBindingKind;
use crate::dependency::Dependency;
use crate::error::RegistrationError;
use crate::options::DefinitionSource;

/// A concrete binding and all its interface aliases, staged atomically.
pub(crate) struct PendingDefinition {
    /// Source attached to every binding declared by this definition.
    pub(crate) source: DefinitionSource,
    /// Optional profile required for activation.
    pub(crate) profile: Option<String>,
    /// Requests that must be selected before executing the concrete factory.
    pub(crate) dependencies: Vec<Dependency>,
    /// Concrete binding followed by its interface aliases.
    pub(crate) bindings: Vec<PendingBinding>,
}

// RegistrationError keeps complete public diagnostic keys and source locations.
#[allow(clippy::result_large_err)]
impl PendingDefinition {
    /// Creates one definition with its concrete binding and declared requests.
    ///
    /// `profile` and IDs must already be validated by the registration entry.
    /// Call [`Self::validate_self_keys`] before adding the definition to a
    /// builder.
    #[must_use]
    pub(crate) fn new(
        source: DefinitionSource,
        profile: Option<String>,
        dependencies: Vec<Dependency>,
        concrete: PendingBinding,
    ) -> Self {
        Self {
            source,
            profile,
            dependencies,
            bindings: vec![concrete],
        }
    }

    /// Adds an interface alias to the pending definition before validation.
    pub(crate) fn add_alias(&mut self, alias: PendingBinding) {
        self.bindings.push(alias);
    }

    /// Rejects a duplicate exact key inside this definition before staging.
    ///
    /// Cross-definition duplicates are checked later after profile filtering.
    pub(crate) fn validate_self_keys(&self) -> Result<(), RegistrationError> {
        let mut keys = HashSet::with_capacity(self.bindings.len());
        for binding in &self.bindings {
            if !keys.insert(&binding.key) {
                return Err(RegistrationError::DuplicateDefinitionKey {
                    key: binding.key.clone(),
                    definition: self.source,
                });
            }
        }
        let concrete = self.bindings.first().ok_or(RegistrationError::EmptyDefinition {
            definition: self.source,
        })?;
        if matches!(concrete.kind, PendingBindingKind::Alias { .. }) {
            return Err(RegistrationError::InvalidConcreteBinding {
                key: concrete.key.clone(),
                definition: self.source,
            });
        }
        for binding in self.bindings.iter().skip(1) {
            match &binding.kind {
                PendingBindingKind::Alias { target, .. } if target != &concrete.key => {
                    return Err(RegistrationError::InvalidAliasTarget {
                        alias: binding.key.clone(),
                        target: target.clone(),
                        expected: concrete.key.clone(),
                        definition: self.source,
                    });
                }
                PendingBindingKind::Alias { .. } => {}
                _ => {
                    return Err(RegistrationError::InvalidAliasBinding {
                        key: binding.key.clone(),
                        definition: self.source,
                    });
                }
            }
        }
        Ok(())
    }

    /// Validates every key before appending this complete definition to
    /// `definitions`.
    ///
    /// On a duplicate exact key, returns its source and leaves the staging area
    /// untouched; different definitions may share keys until graph validation.
    pub(crate) fn stage_into(self, definitions: &mut Vec<Self>) -> Result<(), RegistrationError> {
        self.validate_self_keys()?;
        definitions.push(self);
        Ok(())
    }
}
