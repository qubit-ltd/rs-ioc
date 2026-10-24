// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Verifies how the runtime dependency path resolves `Managed<T>` fields.

use quote::ToTokens;
use syn::Type;
use syn::parse_str;

use crate::internal::RuntimePath;

/// Accepts only bare or exact runtime-qualified managed paths.
#[test]
fn test_recognizes_exact_managed_paths() {
    let runtime = RuntimePath::for_root("ioc");
    for source in ["Managed<u32>", "ioc::Managed<u32>", "::ioc::Managed<u32>"] {
        let ty = parse_str::<Type>(source).unwrap();
        let inner = runtime
            .managed_argument(&ty)
            .unwrap_or_else(|| panic!("{source} should be recognized"));
        assert_eq!(inner.to_token_stream().to_string(), "u32", "{source}");
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

/// Rejects malformed, unrelated, and non-type generic arguments.
#[test]
fn test_rejects_other_managed_path_shapes() {
    let runtime = RuntimePath::for_root("ioc");
    for source in [
        "&u32",
        "<u32 as core::ops::Deref>::Target",
        "::Managed<u32>",
        "Managed",
        "Managed<u32, u64>",
        "Managed<'static>",
        "ioc::Managed",
        "ioc<u8>::Managed<u32>",
    ] {
        let ty = parse_str::<Type>(source).unwrap_or_else(|error| panic!("{source}: {error}"));
        assert!(runtime.managed_argument(&ty).is_none(), "{source}");
    }
}

/// Matches a runtime alias whose spelling is a Rust keyword.
#[test]
fn test_recognizes_raw_runtime_alias() {
    let runtime = RuntimePath::for_root("type");
    let ty = parse_str::<Type>("r#type::Managed<u32>").unwrap();

    assert!(runtime.managed_argument(&ty).is_some());
}
