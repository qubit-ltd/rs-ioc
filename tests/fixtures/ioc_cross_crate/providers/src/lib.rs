// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Provider definitions and an explicit installation list for downstream applications.

use qubit_ioc::ContainerBuilder;
use qubit_ioc::RegistrationError;

mod app_service;
mod disk_repository;
mod greeting;
mod memory_repository;
mod preview_marker;
mod settings;

pub use app_service::AppService;
pub use disk_repository::DiskRepository;
pub use greeting::Greeting;
pub use memory_repository::MemoryRepository;
pub use preview_marker::PreviewMarker;
pub use settings::Settings;


/// Installs every provider definition in this crate into an application builder.
///
/// The list is explicit so the application does not depend on linker retention
/// of registration-only items.
pub fn register_ioc(builder: &mut ContainerBuilder) -> Result<(), RegistrationError> {
    builder.install::<MemoryRepository>()?;
    builder.install::<DiskRepository>()?;
    builder.install::<Settings>()?;
    builder.install::<AppService>()?;
    builder.install::<PreviewMarker>()?;
    greeting::register_ioc(builder)
}
