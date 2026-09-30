// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Deferred alias declarations used during complete-definition validation.

// Holds the alias draft type and its validation function alias.
mod alias_draft;

pub(super) use alias_draft::AliasDraft;
