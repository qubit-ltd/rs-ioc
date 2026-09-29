// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Cleanup callbacks paired with binding metadata.

use super::entry_state::EntryState;
use crate::key::BindingKey;
use crate::managed::CleanupAction;
use crate::options::DefinitionSource;

/// Lifecycle action paired with the public binding metadata used in errors.
pub(crate) struct CleanupEntry {
    /// Exact binding key used to identify this component in errors.
    pub(crate) key: BindingKey,
    /// Definition source retained for shutdown diagnostics.
    pub(crate) definition: DefinitionSource,
    /// One-shot cleanup callbacks associated with the binding.
    pub(crate) action: CleanupAction,
    /// Progress retained by the unique lifecycle owner.
    pub(super) state: EntryState,
}
