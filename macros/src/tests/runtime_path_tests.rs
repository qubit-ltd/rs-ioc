// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Verifies how the runtime dependency path resolves `Managed<T>` fields.

use syn::Type;
use syn::parse_str;

use crate::internal::RuntimePath;

/// Accepts only bare or exact runtime-qualified managed paths.
#[test]
fn test_recognizes_exact_managed_paths() {
    let runtime = RuntimePath::for_root("ioc");
    for source in ["Managed<u32>", "ioc::Managed<u32>", "::ioc::Managed<u32>"] {
        let ty = parse_str::<Type>(source).unwrap();
        assert!(runtime.managed_argument(&ty).is_some(), "{source}");
    }
    for source in [
        "application::Managed<u32>",
        "ioc::nested::Managed<u32>",
        "ioc::Other<u32>",
    ] {
        let ty = parse_str::<Type>(source).unwrap();
        assert!(runtime.managed_argument(&ty).is_none(), "{source}");
    }
}
