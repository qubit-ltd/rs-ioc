// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! In-memory fixture repository.

use qubit_ioc::Component;
use qubit_ioc_fixture_contracts::Repository;

/// Preferred in-memory repository.
#[Component(bind = dyn Repository, id = "fixture.repo.memory", primary, order = 1)]
pub struct MemoryRepository;

impl Repository for MemoryRepository {
    /// Returns a memory-prefixed label for the supplied record identifier.
    fn find(&self, id: u64) -> String {
        format!("memory:{id}")
    }
}
