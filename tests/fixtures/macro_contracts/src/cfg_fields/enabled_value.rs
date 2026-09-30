// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Component whose configuration field activates with the consumer's
//! `config` feature.

use r#type::Component;

/// Exercises an active configuration-backed field with the consumer config
/// feature.
#[cfg(feature = "config")]
#[Component]
pub struct EnabledValue {
    /// Configuration value the expansion always reads while this type exists.
    ///
    /// Unlike the disabled counterpart in `disabled_value.rs`, this field
    /// carries no `cfg` of its own, so whenever the type-level
    /// `config` feature is on, `#[value("test.enabled")]` turns the field
    /// into a live configuration read and the generated factory must be able
    /// to resolve `test.enabled` before the component is built.
    #[value("test.enabled")]
    value: String,
}
