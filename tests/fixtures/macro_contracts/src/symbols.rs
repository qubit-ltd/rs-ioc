// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Checks macro output against consumer-local names and crate aliases.
//!
//! `collision_result` owns the consumer-local shadowing names and the value
//! type they threaten. The `result_shadow` group stays inline because
//! `#[Configuration]` rejects a file-style module: its beans must be direct
//! children of the module the attribute decorates, and `pub(super)` on those
//! beans must keep resolving to this parent.

use r#type::Configuration;

mod collision_result;

pub use collision_result::CollisionBean;
pub use collision_result::OrdinaryBean;

/// Installs a child bean from a module with a local result alias.
#[Configuration]
mod result_shadow {
    use r#type::bean;

    /// Module-local alias that shadows `Result` for the beans below.
    type Result<T> = std::result::Result<T, std::io::Error>;

    /// Registers a plain child bean of the shadowing configuration module.
    #[must_use]
    #[bean]
    pub(super) fn configured() -> u64 {
        11
    }

    /// Registers a child bean only while the `extra` feature is enabled.
    #[cfg_attr(not(feature = "extra"), cfg(any()))]
    #[must_use]
    #[bean]
    pub(super) fn conditional() -> usize {
        13
    }
}
