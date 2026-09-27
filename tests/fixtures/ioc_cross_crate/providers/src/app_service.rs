// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Service assembled from the provider fixture's dependencies.

use std::sync::Arc;

use qubit_ioc::Service;
use qubit_ioc_fixture_contracts::Repository;

use crate::Settings;

/// A service resolved from definitions in this crate and a separate contract.
#[Service]
pub struct AppService {
    /// Preferred repository selected by its primary alias.
    pub primary: Arc<dyn Repository>,
    /// Disk repository selected by its exact binding ID.
    #[inject(id = "fixture.repo.disk")]
    pub disk: Arc<dyn Repository>,
    /// Configuration-derived provider settings.
    pub settings: Arc<Settings>,
}
