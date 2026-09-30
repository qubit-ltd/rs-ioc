// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Component whose configuration field is disabled, so it must not require
//! any configuration.

use r#type::Component;

/// Contains an unavailable configuration value behind a false condition.
#[Component]
pub struct DisabledValue {
    /// Configuration request the expansion must drop together with the field.
    ///
    /// `#[value("unused.port")]` makes the `#[Component]` expansion read a
    /// configuration value for this field. Because the field itself is gated
    /// by the always-false `cfg(any())`, the expansion has to strip the field
    /// before it looks at the attribute, so neither the `unused.port` read nor
    /// any generated configuration requirement may survive. The component must
    /// therefore stay instantiable with no configuration supplied at all.
    #[cfg(any())]
    #[value("unused.port")]
    port: u16,
}
