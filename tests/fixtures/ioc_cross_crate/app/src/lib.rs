// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Application fixture that links providers and performs discovery.

use qubit_config::Config;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::RegistrationError;
use qubit_ioc_fixture_providers::linked_marker;
pub use qubit_ioc_fixture_contracts::Repository;
pub use qubit_ioc_fixture_providers::AppService;
pub use qubit_ioc_fixture_providers::DiskRepository;
pub use qubit_ioc_fixture_providers::Greeting;
pub use qubit_ioc_fixture_providers::MemoryRepository;
pub use qubit_ioc_fixture_providers::PreviewMarker;
pub use qubit_ioc_fixture_providers::Settings;

/// Stages the linked definitions and configuration without running factories.
///
/// `profiles` replaces the default active profile set; invalid profile names
/// or a registration error are returned before graph construction.
pub fn discover(config: Config, profiles: &[&str]) -> Result<ContainerBuilder, RegistrationError> {
    let _ = linked_marker();
    ContainerBuilder::new()
        .active_profiles(profiles)?
        .with_config(config)?
        .discover()
}
