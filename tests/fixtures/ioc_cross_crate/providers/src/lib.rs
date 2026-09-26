// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Linked provider definitions for cross-crate discovery.

use std::sync::Arc;

use qubit_ioc::Component;
use qubit_ioc::ConfigurationProperties;
use qubit_ioc::Service;
use qubit_ioc::bean;
use qubit_ioc_fixture_contracts::Repository;
use serde::Deserialize;

/// Preferred in-memory repository.
#[Component(bind = dyn Repository, id = "fixture.repo.memory", primary, order = 1)]
pub struct MemoryRepository;

impl Repository for MemoryRepository {
    fn find(&self, id: u64) -> String {
        format!("memory:{id}")
    }
}

/// Alternative repository selected by its exact binding ID.
#[Component(bind = dyn Repository, id = "fixture.repo.disk", order = 2)]
pub struct DiskRepository;

impl Repository for DiskRepository {
    fn find(&self, id: u64) -> String {
        format!("disk:{id}")
    }
}

/// Typed settings read from the application's configuration snapshot.
#[derive(Deserialize)]
#[ConfigurationProperties(prefix = "fixture")]
pub struct Settings {
    pub label: String,
}

/// A service resolved from definitions in this crate and contracts in another.
#[Service]
pub struct AppService {
    pub primary: Arc<dyn Repository>,
    #[inject(id = "fixture.repo.disk")]
    pub disk: Arc<dyn Repository>,
    pub settings: Arc<Settings>,
}

/// One bean generated from a function in a linked provider crate.
pub struct Greeting(pub String);

#[bean]
fn greeting(service: Arc<AppService>) -> Greeting {
    Greeting(format!("{}:{}", service.settings.label, service.primary.find(7)))
}

/// A component active only when the `preview` profile is selected.
#[Component(profile = "preview")]
pub struct PreviewMarker;

/// Makes the provider crate an explicit dependency of the fixture application.
pub fn linked_marker() -> &'static str {
    "providers-linked"
}
