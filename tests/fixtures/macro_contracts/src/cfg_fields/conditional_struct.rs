// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Component activated through a `cfg_attr` activation condition.

use r#type::Component;

/// Removes the component when cfg_attr enables a false activation condition.
#[Component]
#[cfg_attr(not(feature = "extra"), cfg(any()), derive(Clone))]
pub struct ConditionalStruct;
