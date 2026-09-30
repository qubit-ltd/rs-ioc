// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Component whose dependency field is gated by a nested activation
//! attribute.

use std::sync::Arc;

use r#type::Component;

/// Uses a nested conditional activation attribute.
#[Component]
pub struct NestedCondition {
    /// Container dependency the `#[Component]` expansion registers and resolves
    /// before constructing this component.
    ///
    /// The field is never self-initialised: the generated factory asks the
    /// container for an `Arc<u16>` and clones it into the struct literal, so a
    /// consumer that enables `extra` must provide exactly one such binding. The
    /// gating differs from the plain `#[cfg]` form used by `conditional.rs`:
    /// `cfg_attr` first evaluates `not(feature = "extra")` and only then
    /// substitutes the nested `cfg(any())`, which disables the attribute rather
    /// than the field directly. With `extra` off, the nested condition is always
    /// false, so neither the struct member nor the container request for
    /// `Arc<u16>` is generated, and that stripping is exactly the contract this
    /// fixture pins down.
    #[cfg_attr(not(feature = "extra"), cfg(any()))]
    extra: Arc<u16>,
}
