// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use std::sync::Arc;

use qubit_ioc::ApplicationContext;
use qubit_ioc::BindingId;
use qubit_ioc::BindingKey;
use qubit_ioc::BindingOptions;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::ResolveError;

#[test]
fn test_get_reuses_shared_arc() {
    let instance = Arc::new(String::from("shared"));
    let mut builder = ApplicationContext::builder();
    builder
        .register_instance(Arc::clone(&instance))
        .expect("stage instance");
    let context = builder.build_all().expect("build context");
    let first = context.get::<String>().expect("first lookup");
    let second = context.get::<String>().expect("second lookup");
    assert!(Arc::ptr_eq(&instance, &first));
    assert!(Arc::ptr_eq(&first, &second));
}

#[test]
fn test_get_by_id_and_get_all_share_instances() {
    let first = Arc::new(11_u32);
    let second = Arc::new(22_u32);
    let mut builder = ContainerBuilder::new();
    builder
        .register_instance_with(
            Arc::clone(&first),
            BindingOptions {
                id: Some("first".into()),
                order: 2,
                ..BindingOptions::default()
            },
        )
        .expect("stage first");
    builder
        .register_instance_with(
            Arc::clone(&second),
            BindingOptions {
                id: Some("second".into()),
                order: 1,
                ..BindingOptions::default()
            },
        )
        .expect("stage second");
    let context = builder.build_all().expect("build context");
    assert!(Arc::ptr_eq(
        &first,
        &context.get_by_id::<u32>("first").expect("named lookup")
    ));
    let all = context.get_all::<u32>();
    assert_eq!(all.iter().map(|value| **value).collect::<Vec<_>>(), vec![22, 11]);
    assert!(matches!(
        context.get::<u32>(),
        Err(ResolveError::AmbiguousBinding { .. })
    ));
}

#[test]
fn test_equal_order_sorts_by_id_and_missing_lookup_lists_available_keys() {
    let mut builder = ContainerBuilder::new();
    for (id, value) in [("zeta", 26_u32), ("alpha", 1_u32)] {
        builder
            .register_instance_with(
                Arc::new(value),
                BindingOptions {
                    id: Some(id.to_owned()),
                    order: 0,
                    ..BindingOptions::default()
                },
            )
            .expect("stage named value");
    }
    let context = builder.build_all().expect("build named values");
    assert_eq!(
        context.get_all::<u32>().iter().map(|value| **value).collect::<Vec<_>>(),
        [1, 26]
    );
    let error = context.get_by_id::<u32>("missing").expect_err("unknown ID must fail");
    match error {
        ResolveError::MissingComponent { request, available } => {
            assert_eq!(request.id().expect("requested ID").as_str(), "missing");
            assert_eq!(available.len(), 2);
            assert!(
                available
                    .iter()
                    .any(|key| key.id().is_some_and(|id| id.as_str() == "alpha"))
            );
            assert!(
                available
                    .iter()
                    .any(|key| key.id().is_some_and(|id| id.as_str() == "zeta"))
            );
        }
        other => panic!("expected missing component, got {other:?}"),
    }
    assert!(matches!(
        context.try_get::<u32>(),
        Err(ResolveError::AmbiguousBinding { .. })
    ));
}

#[test]
fn test_collection_order_and_named_queries_reuse_arcs() {
    let mut builder = ContainerBuilder::new();
    for (id, order, value) in [("z", 0, 3_u8), ("a", 0, 1), ("m", -1, 2)] {
        builder
            .register_instance_with(
                Arc::new(value),
                BindingOptions {
                    id: Some(id.to_owned()),
                    order,
                    ..BindingOptions::default()
                },
            )
            .expect("register collection member");
    }
    let context = builder.build_all().expect("build collection");
    let first = context.get_all::<u8>();
    let second = context.get_all::<u8>();
    assert_eq!(first.iter().map(|value| **value).collect::<Vec<_>>(), [2, 1, 3]);
    assert_eq!(first.len(), second.len());
    assert!(first.iter().zip(&second).all(|(left, right)| Arc::ptr_eq(left, right)));
    let named = context.get_by_id::<u8>("a").expect("named member");
    assert!(Arc::ptr_eq(&named, &first[1]));
    assert!(Arc::ptr_eq(
        &named,
        &context.get_by_id::<u8>("a").expect("repeated named member")
    ));
}

#[test]
fn test_unique_primary_get_and_try_get_reuse_the_same_arc() {
    let mut builder = ContainerBuilder::new();
    for (id, primary, value) in [("z", false, 3_u8), ("a", true, 1), ("m", false, 2)] {
        builder
            .register_instance_with(
                Arc::new(value),
                BindingOptions {
                    id: Some(id.to_owned()),
                    primary,
                    ..BindingOptions::default()
                },
            )
            .expect("register primary selection candidate");
    }
    let context = builder.build_all().expect("build primary selection");
    let selected = context.get::<u8>().expect("unique primary");
    assert_eq!(*selected, 1);
    assert!(Arc::ptr_eq(&selected, &context.get::<u8>().expect("repeated primary")));
    assert!(Arc::ptr_eq(
        &selected,
        &context
            .try_get::<u8>()
            .expect("optional primary")
            .expect("primary exists")
    ));
    assert!(Arc::ptr_eq(
        &selected,
        &context.get_by_id::<u8>("a").expect("named primary")
    ));
}

#[test]
fn test_ambiguity_and_missing_errors_preserve_registered_candidate_order() {
    let mut builder = ContainerBuilder::new();
    let mut expected = Vec::new();
    for (position, id) in ["z", "a", "m"].into_iter().enumerate() {
        builder
            .register_instance_with(
                Arc::new(position as u8),
                BindingOptions {
                    id: Some(id.to_owned()),
                    order: -(position as i32),
                    ..BindingOptions::default()
                },
            )
            .expect("register ambiguous candidate");
        expected.push(BindingKey::of::<u8>(Some(BindingId::parse(id).expect("valid ID"))));
    }
    let context = builder.build_all().expect("build ambiguous candidates");
    for error in [
        context.get::<u8>().expect_err("get is ambiguous"),
        context.try_get::<u8>().expect_err("try_get is equally ambiguous"),
    ] {
        match error {
            ResolveError::AmbiguousBinding { request, candidates } => {
                assert_eq!(request, BindingKey::of::<u8>(None));
                assert_eq!(candidates, expected);
            }
            other => panic!("expected ambiguity, got {other:?}"),
        }
    }
    match context.get_by_id::<u8>("missing").expect_err("unknown ID") {
        ResolveError::MissingComponent { request, available } => {
            assert_eq!(
                request,
                BindingKey::of::<u8>(Some(BindingId::parse("missing").expect("valid missing ID")))
            );
            assert_eq!(available, expected);
        }
        other => panic!("expected missing component, got {other:?}"),
    }
}

#[test]
fn test_absent_type_queries_preserve_empty_contracts() {
    let context = ContainerBuilder::new().build_all().expect("build empty context");
    assert!(context.try_get::<u8>().expect("optional absent type").is_none());
    assert!(context.get_all::<u8>().is_empty());
    match context.get::<u8>().expect_err("absent type") {
        ResolveError::MissingComponent { request, available } => {
            assert_eq!(request, BindingKey::of::<u8>(None));
            assert!(available.is_empty());
        }
        other => panic!("expected missing component, got {other:?}"),
    }
}
