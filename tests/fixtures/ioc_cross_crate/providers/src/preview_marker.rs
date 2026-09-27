// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Profile-gated marker component for the provider fixture.

use qubit_ioc::Component;

/// A component active only when the `preview` profile is selected.
#[Component(profile = "preview")]
pub struct PreviewMarker;
