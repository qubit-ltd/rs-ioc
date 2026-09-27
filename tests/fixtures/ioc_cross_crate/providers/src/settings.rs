// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Configuration-backed provider settings.

use qubit_ioc::ConfigurationProperties;
use serde::Deserialize;

/// Typed settings read from the application's configuration snapshot.
#[derive(Deserialize)]
#[ConfigurationProperties(prefix = "fixture")]
pub struct Settings {
    /// Display label used by the fixture greeting.
    pub label: String,
}
