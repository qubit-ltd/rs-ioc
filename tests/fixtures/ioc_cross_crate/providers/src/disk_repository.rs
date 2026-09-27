// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Disk-backed fixture repository.

use qubit_ioc::Component;
use qubit_ioc_fixture_contracts::Repository;

/// Alternative repository selected by its exact binding ID.
#[Component(bind = dyn Repository, id = "fixture.repo.disk", order = 2)]
pub struct DiskRepository;

impl Repository for DiskRepository {
    /// Returns a disk-prefixed label for the supplied record identifier.
    fn find(&self, id: u64) -> String {
        format!("disk:{id}")
    }
}
