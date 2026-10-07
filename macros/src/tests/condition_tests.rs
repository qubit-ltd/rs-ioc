// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use quote::ToTokens;
use quote::quote;
use syn::Meta;
use syn::parse_quote;

use crate::conditions::activation_attributes;

#[test]
fn test_cfg_attr_condition_becomes_implication_predicate() {
    let attributes = vec![parse_quote!(#[cfg_attr(feature = "a", cfg(feature = "b"))])];
    let normalized = activation_attributes(&attributes).expect("cfg_attr should normalize");
    let [attribute] = normalized.as_slice() else {
        panic!("one cfg predicate was expected");
    };
    let predicate = attribute.parse_args::<Meta>().expect("cfg predicate should parse");

    assert_eq!(
        predicate.to_token_stream().to_string(),
        quote!(any(not(feature = "a"), feature = "b")).to_string(),
    );
}

#[test]
fn test_cfg_attr_rejects_inject_helper() {
    let attributes = vec![parse_quote!(#[cfg_attr(feature = "a", inject)])];
    let error = match activation_attributes(&attributes) {
        Err(error) => error,
        Ok(_) => panic!("cfg_attr containing inject should fail"),
    };

    assert!(error.to_string().contains("cannot contain"), "{error}");
}
