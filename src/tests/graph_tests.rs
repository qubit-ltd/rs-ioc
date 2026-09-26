// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Graph validation contracts that are unavailable through the public builder.

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use crate::binding::PendingBinding;
use crate::binding::PendingDefinition;
use crate::dependency::Dependency;
use crate::error::BuildError;
use crate::graph::ValidatedGraph;
use crate::key::BindingId;
use crate::key::BindingKey;
use crate::options::DefinitionSource;

struct A;
struct B;
struct C;

/// Creates a source with a stable item name and location for graph assertions.
fn source(item: &'static str) -> DefinitionSource {
    DefinitionSource::new("ioc", "graph_tests", "graph_tests.rs", 1, 1, item)
}

/// Creates an instance definition; graph validation must never invoke a
/// factory.
fn instance<T: Send + Sync + 'static>(
    item: &'static str,
    value: T,
    dependencies: Vec<Dependency>,
    id: Option<&str>,
    primary: bool,
    order: i32,
    profile: Option<&str>,
) -> PendingDefinition {
    let id = id.map(|text| BindingId::parse(text).expect("test ID is valid"));
    let key = BindingKey::of::<T>(id);
    let binding = PendingBinding::instance(key, Arc::new(value), primary, order);
    PendingDefinition::new(source(item), profile.map(str::to_owned), dependencies, binding)
}

/// Returns the item names in construction order, including aliases.
fn ordered_names(graph: &ValidatedGraph) -> Vec<&'static str> {
    graph
        .order
        .iter()
        .map(|location| graph.definitions[location.definition].source.item)
        .collect()
}

/// Returns the key selected by the first dependency of the first definition.
fn selected_key(graph: &ValidatedGraph) -> Option<&BindingKey> {
    graph.resolved.first()?.first()?.keys.first()
}

#[test]
fn test_graph_required_zero_one_and_many_candidates() {
    let consumer = || instance("A", A, vec![Dependency::of::<B>()], None, false, 0, None);
    let missing = ValidatedGraph::validate(vec![consumer()], &[]);
    assert!(
        matches!(missing, Err(BuildError::MissingDependency { path, .. }) if path == vec![BindingKey::of::<A>(None)])
    );

    let one = ValidatedGraph::validate(vec![consumer(), instance("B", B, vec![], None, false, 0, None)], &[])
        .expect("one candidate resolves");
    assert_eq!(selected_key(&one), Some(&BindingKey::of::<B>(None)));

    let many = ValidatedGraph::validate(
        vec![
            consumer(),
            instance("B1", B, vec![], Some("first"), false, 0, None),
            instance("B2", B, vec![], Some("second"), false, 0, None),
        ],
        &[],
    );
    assert!(matches!(many, Err(BuildError::AmbiguousBinding { candidates, .. }) if candidates.len() == 2));
}

#[test]
fn test_graph_primary_selects_unique_and_rejects_multiple_without_request() {
    let selected = ValidatedGraph::validate(
        vec![
            instance("A", A, vec![Dependency::of::<B>()], None, false, 0, None),
            instance("B1", B, vec![], Some("first"), false, 0, None),
            instance("B2", B, vec![], Some("second"), true, 0, None),
        ],
        &[],
    )
    .expect("unique primary resolves");
    assert_eq!(
        selected_key(&selected),
        Some(&BindingKey::of::<B>(Some(
            BindingId::parse("second").expect("valid ID")
        )))
    );

    let duplicated = ValidatedGraph::validate(
        vec![
            instance("B1", B, vec![], Some("first"), true, 0, None),
            instance("B2", B, vec![], Some("second"), true, 0, None),
        ],
        &[],
    );
    assert!(matches!(duplicated, Err(BuildError::MultiplePrimaryBindings { candidates }) if candidates.len() == 2));
}

#[test]
fn test_graph_id_is_exact_and_ignores_primary() {
    let graph = ValidatedGraph::validate(
        vec![
            instance("A", A, vec![Dependency::with_id::<B>("target")], None, false, 0, None),
            instance("B1", B, vec![], Some("other"), true, 0, None),
            instance("B2", B, vec![], Some("target"), false, 0, None),
        ],
        &[],
    )
    .expect("exact ID resolves independently of primary");
    assert_eq!(
        selected_key(&graph),
        Some(&BindingKey::of::<B>(Some(
            BindingId::parse("target").expect("valid ID")
        )))
    );

    let missing = ValidatedGraph::validate(
        vec![
            instance("A", A, vec![Dependency::with_id::<B>("absent")], None, false, 0, None),
            instance("B", B, vec![], Some("other"), true, 0, None),
        ],
        &[],
    );
    assert!(matches!(missing, Err(BuildError::MissingDependency { .. })));
}

#[test]
fn test_graph_optional_and_all_allow_empty_and_add_matched_edges() {
    let empty = ValidatedGraph::validate(
        vec![instance(
            "A",
            A,
            vec![Dependency::optional::<B>(), Dependency::all::<C>()],
            None,
            false,
            0,
            None,
        )],
        &[],
    )
    .expect("empty optional and collection are valid");
    assert!(empty.resolved[0].iter().all(|request| request.keys.is_empty()));

    let graph = ValidatedGraph::validate(
        vec![
            instance(
                "A",
                A,
                vec![Dependency::optional::<B>(), Dependency::all::<C>()],
                None,
                false,
                0,
                None,
            ),
            instance("B", B, vec![], None, false, 0, None),
            instance("C1", C, vec![], Some("z"), false, 1, None),
            instance("C2", C, vec![], Some("a"), false, -1, None),
        ],
        &[],
    )
    .expect("all matched edges resolve");
    assert_eq!(graph.resolved[0][0].keys.len(), 1);
    assert_eq!(
        graph.resolved[0][1].keys,
        vec![
            BindingKey::of::<C>(Some(BindingId::parse("a").expect("valid ID"))),
            BindingKey::of::<C>(Some(BindingId::parse("z").expect("valid ID"))),
        ]
    );
    assert_eq!(ordered_names(&graph).last(), Some(&"A"));
}

#[test]
fn test_graph_profiles_filter_before_duplicate_check() {
    let definitions = || {
        vec![
            instance("prod", B, vec![], None, false, 0, Some("prod")),
            instance("test", B, vec![], None, false, 0, Some("test")),
        ]
    };
    let one = ValidatedGraph::validate(definitions(), &["prod".to_owned()]).expect("inactive duplicate is ignored");
    assert_eq!(ordered_names(&one), vec!["prod"]);
    let both = ValidatedGraph::validate(definitions(), &["prod".to_owned(), "test".to_owned()]);
    assert!(
        matches!(both, Err(BuildError::DuplicateBinding { first, second, .. }) if first.item == "prod" && second.item == "test")
    );
}

#[test]
fn test_graph_cycle_and_dependency_first_stable_topology() {
    let graph = ValidatedGraph::validate(
        vec![
            instance("A", A, vec![Dependency::of::<B>()], None, false, 0, None),
            instance("B", B, vec![Dependency::of::<C>()], None, false, 0, None),
            instance("C", C, vec![], None, false, 0, None),
        ],
        &[],
    )
    .expect("acyclic chain validates");
    assert_eq!(ordered_names(&graph), vec!["C", "B", "A"]);

    let cycle = ValidatedGraph::validate(
        vec![
            instance("A", A, vec![Dependency::of::<B>()], None, false, 0, None),
            instance("B", B, vec![Dependency::of::<A>()], None, false, 0, None),
        ],
        &[],
    );
    assert!(
        matches!(cycle, Err(BuildError::DependencyCycle { path }) if path == vec![BindingKey::of::<A>(None), BindingKey::of::<B>(None), BindingKey::of::<A>(None)])
    );
}

#[test]
fn test_graph_keeps_registration_order_for_independent_bindings() {
    let definitions = (0..128)
        .map(|index| {
            instance(
                "independent",
                index,
                vec![],
                Some(&format!("node.n{index}")),
                false,
                0,
                None,
            )
        })
        .collect();
    let graph = ValidatedGraph::validate(definitions, &[]).expect("independent bindings validate");
    let ids: Vec<_> = graph
        .order
        .iter()
        .map(|location| {
            graph.definitions[location.definition].bindings[location.binding]
                .key
                .id()
                .expect("each binding has an ID")
                .as_str()
                .to_owned()
        })
        .collect();
    let expected: Vec<_> = (0..128).map(|index| format!("node.n{index}")).collect();
    assert_eq!(ids, expected);
}

#[test]
fn test_graph_cycle_path_retains_root_before_repeated_non_root_node() {
    let cycle = ValidatedGraph::validate(
        vec![
            instance("A", A, vec![Dependency::of::<B>()], None, false, 0, None),
            instance("B", B, vec![Dependency::of::<C>()], None, false, 0, None),
            instance("C", C, vec![Dependency::of::<B>()], None, false, 0, None),
        ],
        &[],
    );
    assert!(
        matches!(cycle, Err(BuildError::DependencyCycle { path }) if path == vec![
            BindingKey::of::<A>(None),
            BindingKey::of::<B>(None),
            BindingKey::of::<C>(None),
            BindingKey::of::<B>(None),
        ])
    );
}

#[test]
fn test_graph_collection_orders_by_order_id_then_source() {
    let graph = ValidatedGraph::validate(
        vec![
            instance("A", A, vec![Dependency::all::<B>()], None, false, 0, None),
            instance("z", B, vec![], Some("z"), false, 0, None),
            instance("unnamed", B, vec![], None, false, 0, None),
            instance("low", B, vec![], Some("low"), false, -2, None),
            instance("a", B, vec![], Some("a"), false, 0, None),
        ],
        &[],
    )
    .expect("collection candidates resolve");
    let ids: Vec<Option<&str>> = graph.resolved[0][0]
        .keys
        .iter()
        .map(|key| key.id().map(BindingId::as_str))
        .collect();
    assert_eq!(ids, vec![Some("low"), None, Some("a"), Some("z")]);
}

#[test]
fn test_graph_error_path_follows_root_and_declared_dependency_order() {
    let result = ValidatedGraph::validate(
        vec![
            instance(
                "A",
                A,
                vec![Dependency::of::<B>(), Dependency::of::<C>()],
                None,
                false,
                0,
                None,
            ),
            instance("B", B, vec![Dependency::with_id::<C>("missing")], None, false, 0, None),
        ],
        &[],
    );
    assert!(
        matches!(result, Err(BuildError::MissingDependency { path, definition, .. }) if definition.item == "B" && path == vec![BindingKey::of::<A>(None), BindingKey::of::<B>(None)])
    );
}

#[test]
fn test_graph_alias_edges_detect_cycle() {
    let mut first = instance("first", A, vec![], None, false, 0, None);
    first.add_alias(PendingBinding::alias::<String, String, _>(
        BindingKey::of::<String>(Some(BindingId::parse("first").expect("valid ID"))),
        BindingKey::of::<String>(Some(BindingId::parse("second").expect("valid ID"))),
        false,
        0,
        |value| value,
    ));
    let mut second = instance("second", B, vec![], None, false, 0, None);
    second.add_alias(PendingBinding::alias::<String, String, _>(
        BindingKey::of::<String>(Some(BindingId::parse("second").expect("valid ID"))),
        BindingKey::of::<String>(Some(BindingId::parse("first").expect("valid ID"))),
        false,
        0,
        |value| value,
    ));
    let result = ValidatedGraph::validate(vec![first, second], &[]);
    assert!(matches!(result, Err(BuildError::DependencyCycle { path }) if path.len() == 3));
}

#[test]
fn test_graph_missing_alias_target_is_structured() {
    let mut definition = instance("A", A, vec![], None, false, 0, None);
    let alias = BindingKey::of::<String>(Some(BindingId::parse("alias").expect("valid ID")));
    let target = BindingKey::of::<String>(Some(BindingId::parse("missing").expect("valid ID")));
    definition.add_alias(PendingBinding::alias::<String, String, _>(
        alias.clone(),
        target.clone(),
        false,
        0,
        |value| value,
    ));
    let result = ValidatedGraph::validate(vec![definition], &[]);
    assert!(matches!(result, Err(BuildError::MissingAliasTarget {
        alias: actual_alias, target: actual_target, definition, path,
    }) if actual_alias == alias && actual_target == target && definition.item == "A" && path == vec![alias]));
}

#[test]
fn test_graph_ready_bindings_use_registration_order() {
    let graph = ValidatedGraph::validate(
        vec![
            instance("A", A, vec![Dependency::of::<C>()], None, false, 0, None),
            instance("B", B, vec![], None, false, 0, None),
            instance("C", C, vec![], None, false, 0, None),
        ],
        &[],
    )
    .expect("independent nodes have stable order");
    assert_eq!(ordered_names(&graph), vec!["B", "C", "A"]);
}

#[test]
fn test_graph_validation_does_not_call_factories() {
    let calls = Arc::new(AtomicUsize::new(0));
    let factory_calls = Arc::clone(&calls);
    let factory = PendingBinding::sync_factory::<B, _>(BindingKey::of::<B>(None), false, 0, move |_| {
        factory_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(B))
    });
    let definition = PendingDefinition::new(source("B"), None, vec![], factory);
    ValidatedGraph::validate(vec![definition], &[]).expect("factory graph validates");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
