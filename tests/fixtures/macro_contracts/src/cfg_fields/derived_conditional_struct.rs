// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Component that keeps a non-activation derive while cfg_attr keeps it
//! active.

use r#type::Component;

/// Retains a non-activation derive when cfg_attr selects an active component.
#[Component]
#[cfg_attr(feature = "extra", cfg(all()), derive(Clone))]
pub struct DerivedConditionalStruct;
