// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Component removed as a whole under a false condition.

use r#type::Component;

/// Removes the entire component, including registration, under a false cfg.
#[Component]
#[cfg(any())]
pub struct DisabledStruct;
