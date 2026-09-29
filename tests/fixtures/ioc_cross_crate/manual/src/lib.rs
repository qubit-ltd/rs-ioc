// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Standalone manual assembly path with no default IoC features.

mod build_manual;
mod manual_error;

pub use build_manual::build_manual;
pub use manual_error::ManualError;

#[cfg(test)]
mod tests {
    use crate::build_manual;

    #[test]
    fn test_manual_assembly_without_default_features() {
        let application = build_manual().expect("build explicit graph");
        let context = application.context();
        assert_eq!(*context.get::<u64>().expect("factory result"), 42);
    }
}
