// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Registration setup for the cross-crate application fixture.

use qubit_config::Config;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::RegistrationError;
use qubit_ioc_fixture_providers::register_ioc;

/// Stages an explicit provider list and configuration without running factories.
///
/// `profiles` replaces the default active profile set; invalid profile names
/// or registration errors are returned before graph construction.
pub fn assemble(config: Config, profiles: &[&str]) -> Result<ContainerBuilder, RegistrationError> {
    let mut builder = ContainerBuilder::new().active_profiles(profiles)?.with_config(config)?;
    register_ioc(&mut builder)?;
    Ok(builder)
}
