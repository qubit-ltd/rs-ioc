// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use qubit_ioc::Component;

trait Repository: Send + Sync {}

#[Component(bind = dyn Repository)]
struct NotARepository;

fn main() {}

// qubit-style: allow test-file-name
