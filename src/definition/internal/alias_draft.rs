// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! An alias binding paired with options awaiting final validation.

use crate::binding::PendingBinding;
use crate::error::RegistrationError;
use crate::key::BindingKey;
use crate::options::BindingOptions;
use crate::options::DefinitionSource;

/// Validates an alias key, profile, and source before registration.
pub(in crate::definition) type AliasValidator =
    fn(&BindingOptions, DefinitionSource) -> Result<(BindingKey, Option<String>), RegistrationError>;

/// Keeps alias options until the concrete profile and key are final.
pub(in crate::definition) struct AliasDraft {
    /// ID, profile, and selection metadata for the alias.
    pub(in crate::definition) options: BindingOptions,
    /// Validates options in the interface type namespace.
    pub(in crate::definition) validate: AliasValidator,
    /// Deferred projection with its typed alias key.
    pub(in crate::definition) binding: PendingBinding,
}
