// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

// This trybuild fixture verifies that a component field with an unsupported type is rejected.

use qubit_ioc::Component;

#[Component]
struct InvalidField {
    name: String,
}

fn main() {}

