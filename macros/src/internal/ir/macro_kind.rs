// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Macro kind macro intermediate representation.

/// The attribute spelling that originated a declaration and its diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MacroKind {
    /// Struct component declaration.
    Component,
    /// Struct service declaration.
    Service,
    /// Struct repository declaration.
    Repository,
    /// Factory function declaration.
    Bean,
    /// Inline registration group declaration.
    Configuration,
    /// Configuration subtree component declaration.
    ConfigurationProperties,
}

impl MacroKind {
    /// Returns the user's attribute spelling for diagnostic messages.
    #[must_use]
    #[inline]
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Component => "Component",
            Self::Service => "Service",
            Self::Repository => "Repository",
            Self::Bean => "bean",
            Self::Configuration => "Configuration",
            Self::ConfigurationProperties => "ConfigurationProperties",
        }
    }
}
