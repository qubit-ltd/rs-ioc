// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Configuration target details retained with generated factory failures.

/// Path and destination retained for a generated configuration read.
#[derive(Debug)]
pub(in crate::error) struct ConfigReadContext {
    /// Configuration key or subtree prefix that failed to read.
    pub(in crate::error) path_key: String,
    /// Generated field, parameter, or properties type being built.
    pub(in crate::error) target: String,
}
