// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Published-context queries with setup and correctness outside timing.

use std::hint::black_box;
use std::sync::Arc;

use qubit_ioc::Application;
use qubit_ioc::ApplicationContext;
use qubit_ioc::BindingId;
use qubit_ioc::BindingKey;
use qubit_ioc::BindingOptions;
use qubit_ioc::ContainerBuilder;

use crate::SIZES;
use crate::measure;

const LOOKUP_ITERATIONS: usize = 10_000;

struct Marker<const INDEX: usize>;

/// Publishes contexts before timing query calls; registration is not measured.
pub(super) fn run() {
    for unrelated in SIZES {
        for candidates in [1_u32, 8, 64] {
            let primary = (candidates > 1).then_some(0);
            let application = build_context(unrelated as u32, candidates, primary);
            benchmark_queries(application.context(), unrelated as u32, candidates, "unique_or_primary");
            if candidates > 1 {
                let ambiguous = build_context(unrelated as u32, candidates, None);
                benchmark_ambiguity(ambiguous.context(), unrelated as u32, candidates);
            }
        }
    }
}

/// Checks exact values, candidate order, source presence, and error cases
/// untimed.
pub(super) fn check() {
    for unrelated in SIZES {
        for candidates in [1_u32, 8, 64] {
            let application = build_context(unrelated as u32, candidates, (candidates > 1).then_some(0));
            let context = application.context();
            assert_eq!(
                black_box(context).get::<String>().expect("unique or primary").as_str(),
                "target-0"
            );
            assert_eq!(
                black_box(context)
                    .try_get::<String>()
                    .expect("valid selection")
                    .expect("present")
                    .as_str(),
                "target-0"
            );
            let id = format!("target.t{}", candidates - 1);
            assert_eq!(
                context.get_by_id::<String>(&id).expect("last target").as_str(),
                format!("target-{}", candidates - 1)
            );
            let all = black_box(context).get_all::<String>();
            assert_eq!(all.len(), candidates as usize);
            for (index, value) in all.iter().enumerate() {
                assert_eq!(value.as_str(), format!("target-{index}"));
            }
            assert!(context.get_by_id::<String>("target.missing").is_err());
            let key = BindingKey::of::<String>(Some(BindingId::parse(&id).expect("target ID")));
            assert!(context.binding_sources(&key).is_some());
            assert!(context.binding_sources(&unrelated_key(unrelated as u32 - 1)).is_some());
            if candidates > 1 {
                let ambiguous = build_context(unrelated as u32, candidates, None);
                assert!(ambiguous.context().get::<String>().is_err());
                assert!(ambiguous.context().try_get::<String>().is_err());
            }
            println!("correctness query unrelated={unrelated} candidates={candidates} passed");
        }
    }
}

/// Registers candidates/markers and publishes an owner; panics on invalid
/// setup.
fn build_context(unrelated: u32, candidates: u32, primary: Option<u32>) -> Application {
    let mut builder = ContainerBuilder::new();
    for index in 0..candidates {
        builder
            .register_instance_with(
                Arc::new(format!("target-{index}")),
                BindingOptions {
                    id: Some(format!("target.t{index}")),
                    primary: primary == Some(index),
                    order: index as i32,
                    ..BindingOptions::default()
                },
            )
            .expect("register target candidate");
    }
    for index in 0..unrelated {
        register_unrelated(&mut builder, index);
    }
    builder.build_all().expect("build benchmark context")
}

/// Registers one marker in sixteen unrelated type namespaces.
fn register_unrelated(builder: &mut ContainerBuilder, index: u32) {
    macro_rules! register_marker {
        ($marker:literal) => {
            builder
                .register_instance_with(
                    Arc::new(Marker::<$marker>),
                    BindingOptions {
                        id: Some(format!("unrelated.u{index}")),
                        ..BindingOptions::default()
                    },
                )
                .expect("register unrelated marker")
        };
    }
    match index % 16 {
        0 => register_marker!(0),
        1 => register_marker!(1),
        2 => register_marker!(2),
        3 => register_marker!(3),
        4 => register_marker!(4),
        5 => register_marker!(5),
        6 => register_marker!(6),
        7 => register_marker!(7),
        8 => register_marker!(8),
        9 => register_marker!(9),
        10 => register_marker!(10),
        11 => register_marker!(11),
        12 => register_marker!(12),
        13 => register_marker!(13),
        14 => register_marker!(14),
        15 => register_marker!(15),
        _ => unreachable!("modulo 16 must stay below 16"),
    }
}

/// Returns the exact marker key corresponding to a registration index.
fn unrelated_key(index: u32) -> BindingKey {
    macro_rules! marker_key {
        ($marker:literal) => {
            BindingKey::of::<Marker<$marker>>(Some(
                BindingId::parse(&format!("unrelated.u{index}")).expect("valid unrelated marker ID"),
            ))
        };
    }
    match index % 16 {
        0 => marker_key!(0),
        1 => marker_key!(1),
        2 => marker_key!(2),
        3 => marker_key!(3),
        4 => marker_key!(4),
        5 => marker_key!(5),
        6 => marker_key!(6),
        7 => marker_key!(7),
        8 => marker_key!(8),
        9 => marker_key!(9),
        10 => marker_key!(10),
        11 => marker_key!(11),
        12 => marker_key!(12),
        13 => marker_key!(13),
        14 => marker_key!(14),
        15 => marker_key!(15),
        _ => unreachable!("modulo 16 must stay below 16"),
    }
}

/// Times successful/absent queries using an already published context.
fn benchmark_queries(context: &ApplicationContext, unrelated: u32, candidates: u32, scenario: &str) {
    let id = format!("target.t{}", candidates - 1);
    let key = BindingKey::of::<String>(Some(BindingId::parse(&id).expect("valid target ID")));
    let missing_id = "target.missing";
    let label = |query: &str| format!("unrelated={unrelated} candidates={candidates} {scenario} {query}");

    measure(&label("get"), LOOKUP_ITERATIONS, || {
        let _ = black_box(black_box(context).get::<String>());
    });
    measure(&label("try_get"), LOOKUP_ITERATIONS, || {
        let _ = black_box(black_box(context).try_get::<String>());
    });
    measure(&label("get_by_id"), LOOKUP_ITERATIONS, || {
        let _ = black_box(context.get_by_id::<String>(black_box(&id)));
    });
    measure(&label("missing_by_id"), LOOKUP_ITERATIONS, || {
        let _ = black_box(context.get_by_id::<String>(black_box(missing_id)));
    });
    measure(&label("get_all"), LOOKUP_ITERATIONS / 100, || {
        let _ = black_box(black_box(context).get_all::<String>());
    });
    measure(&label("binding_sources"), LOOKUP_ITERATIONS, || {
        let _ = black_box(context.binding_sources(black_box(&key)));
    });
    if unrelated > 0 {
        let late_key = unrelated_key(unrelated - 1);
        measure(&label("binding_sources_late_key"), LOOKUP_ITERATIONS, || {
            let _ = black_box(context.binding_sources(black_box(&late_key)));
        });
    }
}

/// Times ambiguous selection with no primary candidate, without timed
/// assertions.
fn benchmark_ambiguity(context: &ApplicationContext, unrelated: u32, candidates: u32) {
    measure(
        &format!("unrelated={unrelated} candidates={candidates} ambiguous get"),
        LOOKUP_ITERATIONS,
        || {
            let _ = black_box(black_box(context).get::<String>());
        },
    );
    measure(
        &format!("unrelated={unrelated} candidates={candidates} ambiguous try_get"),
        LOOKUP_ITERATIONS,
        || {
            let _ = black_box(black_box(context).try_get::<String>());
        },
    );
}
