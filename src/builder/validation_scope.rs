// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Scope of static graph validation for root-scoped construction.

/// Selects which active definitions are checked before building requested
/// roots.
///
/// Both modes construct only the root dependency closure. `AllActive` checks
/// every active definition after profile filtering and replacement, without
/// running unselected factories or requiring their async mode or wait policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidationScope {
    /// Validate only definitions reachable from selected roots.
    #[default]
    Reachable,
    /// Validate every active definition before selecting the root closure.
    AllActive,
}
