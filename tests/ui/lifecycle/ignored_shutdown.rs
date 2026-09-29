// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
#![deny(unused_must_use)]

use qubit_ioc::ShutdownMode;
use qubit_ioc::Application;

fn ignored(application: Application) {
    application.begin_shutdown(ShutdownMode::Immediate);
}

fn main() {}

// qubit-style: allow test-file-name
// This is a trybuild source fixture, not a test module.
