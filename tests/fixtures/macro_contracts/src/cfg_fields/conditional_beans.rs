// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Bean factories whose parameters and values follow the consumer's
//! conditional features.

use std::sync::Arc;

use r#type::bean;

/// Activates a bean dependency only when the matching function parameter
/// exists.
///
/// The `#[bean]` expansion registers the generated `ConditionalFactory`
/// definition and keeps this function callable, so the parameter list below is
/// the macro contract this fixture pins down.
///
/// # Parameters
///
/// * `value` - Injected `Arc<u8>` dependency. Present only while the
///   consumer's `extra` feature is enabled; with the feature off the parameter
///   is stripped from the signature and the generated factory takes no
///   dependency at all.
///
/// # Returns
///
/// The constant `17`. The value never depends on `value`, never reads
/// configuration, and the function never panics or fails.
#[must_use]
#[bean(marker = ConditionalFactory)]
pub fn conditional_bean(#[cfg(feature = "extra")] value: Arc<u8>) -> u32 {
    #[cfg(feature = "extra")]
    let _ = value;
    17
}

/// Requests a configuration value only when its parameter is enabled.
///
/// The `#[bean]` expansion registers the generated `ConditionalConfigFactory`
/// definition and keeps this function callable, so the parameter list below is
/// the macro contract this fixture pins down.
///
/// # Parameters
///
/// * `value` - Configuration value read from the `test.conditional` key. Present
///   only while the consumer's `value_input` feature is enabled; with the
///   feature off both the parameter and the generated configuration read
///   disappear, so the factory requires no configuration at all.
///
/// # Returns
///
/// The constant `19`. The value never depends on the configuration read, and
/// the function itself never panics or fails.
#[must_use]
#[bean(marker = ConditionalConfigFactory)]
pub fn conditional_config_bean(
    #[cfg(feature = "value_input")]
    #[value("test.conditional")]
    value: String,
) -> u32 {
    #[cfg(feature = "value_input")]
    let _ = value;
    19
}
