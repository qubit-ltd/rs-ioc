// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
#![deny(missing_docs)]
#![allow(dead_code)]
//! External consumer contracts for `qubit-ioc` macros.

// Owns the conditional activation contracts driven by the consumer features.
mod cfg_fields;
// Owns the consumer-local names that shadow `Result` and the `Option` variants.
mod symbols;

#[cfg(test)]
mod tests;
