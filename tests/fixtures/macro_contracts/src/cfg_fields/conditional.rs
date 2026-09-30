// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Component whose dependency field activates with the consumer's `extra`
//! feature.

use std::sync::Arc;

use r#type::Component;

/// Includes a dependency only when the consumer enables `extra`.
#[Component]
pub struct Conditional {
    /// Container dependency the `#[Component]` expansion registers and resolves
    /// before constructing this component.
    ///
    /// The field is not self-initialised: the generated factory resolves an
    /// `Arc<u8>` from the container and clones it into the struct literal, so
    /// every consumer must provide exactly one such binding. When the `extra`
    /// feature is off the field is stripped before expansion and neither the
    /// struct member nor the container request for `Arc<u8>` is generated, which
    /// is exactly the contract this fixture pins down.
    #[cfg(feature = "extra")]
    extra: Arc<u8>,
}
