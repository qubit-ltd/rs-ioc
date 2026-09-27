// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Application fixture that explicitly assembles provider definitions.

pub use qubit_ioc_fixture_contracts::Repository;
pub use qubit_ioc_fixture_providers::AppService;
pub use qubit_ioc_fixture_providers::DiskRepository;
pub use qubit_ioc_fixture_providers::Greeting;
pub use qubit_ioc_fixture_providers::MemoryRepository;
pub use qubit_ioc_fixture_providers::PreviewMarker;
pub use qubit_ioc_fixture_providers::Settings;

mod discovery;
pub use discovery::assemble;
