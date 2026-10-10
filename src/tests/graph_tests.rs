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
use crate::builder::ValidationScope;
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
fn test_graph_profile_filter_defaults_and_explicit_selection() {
    let cases: [(Option<&str>, &[&str], bool); 6] = [
        (None, &[], true),
        (Some("default"), &[], true),
        (Some("prod"), &[], false),
        (Some("default"), &["prod"], false),
        (Some("prod"), &["prod"], true),
        (None, &["prod"], true),
    ];
    for (profile, active_profiles, expected) in cases {
        let active_profiles: Vec<_> = active_profiles.iter().map(|profile| (*profile).to_owned()).collect();
        let graph = ValidatedGraph::validate(
            vec![instance("profiled", B, vec![], None, false, 0, profile)],
            &active_profiles,
        )
        .expect("profile filtering validates");
        assert_eq!(
            !graph.order.is_empty(),
            expected,
            "profile {profile:?}, active {active_profiles:?}"
        );
    }
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
    assert!(matches!(
        both,
        Err(BuildError::DuplicateBinding { first, second, .. })
            if first.item == "prod" && second.item == "test"
    ));
}

#[test]
fn test_graph_validates_deep_chain_without_recursion() {
    const NODE_COUNT: usize = 10_000;
    let definitions = (0..NODE_COUNT)
        .map(|index| {
            let dependencies = if index + 1 < NODE_COUNT {
                vec![Dependency::with_id::<u32>(&format!("node.n{}", index + 1))]
            } else {
                Vec::new()
            };
            instance(
                "deep node",
                index as u32,
                dependencies,
                Some(&format!("node.n{index}")),
                false,
                0,
                None,
            )
        })
        .collect();

    let graph = ValidatedGraph::validate(definitions, &[]).expect("deep chain validates");
    assert_eq!(graph.order.len(), NODE_COUNT);
}

#[test]
fn test_graph_reports_full_missing_path_for_deep_chain() {
    const NODE_COUNT: usize = 10_000;
    let definitions = (0..NODE_COUNT)
        .map(|index| {
            let dependencies = if index + 1 < NODE_COUNT {
                vec![Dependency::with_id::<u32>(&format!("node.n{}", index + 1))]
            } else {
                vec![Dependency::of::<C>()]
            };
            instance(
                "deep node",
                index as u32,
                dependencies,
                Some(&format!("node.n{index}")),
                false,
                0,
                None,
            )
        })
        .collect();

    let error = match ValidatedGraph::validate(definitions, &[]) {
        Ok(_) => panic!("missing dependency must fail"),
        Err(error) => error,
    };
    assert!(matches!(error, BuildError::MissingDependency { path, .. } if path.len() == NODE_COUNT));
}

#[test]
fn test_graph_reports_cycle_for_deep_chain() {
    const NODE_COUNT: usize = 10_000;
    let definitions = (0..NODE_COUNT)
        .map(|index| {
            let target = if index + 1 < NODE_COUNT { index + 1 } else { 0 };
            let dependencies = vec![Dependency::with_id::<u32>(&format!("node.n{target}"))];
            instance(
                "deep node",
                index as u32,
                dependencies,
                Some(&format!("node.n{index}")),
                false,
                0,
                None,
            )
        })
        .collect();

    let error = match ValidatedGraph::validate(definitions, &[]) {
        Ok(_) => panic!("back edge must form a cycle"),
        Err(error) => error,
    };
    assert!(matches!(error, BuildError::DependencyCycle { path } if path.len() == NODE_COUNT + 1));
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
    assert!(matches!(
        cycle,
        Err(BuildError::DependencyCycle { path })
            if path == vec![BindingKey::of::<A>(None), BindingKey::of::<B>(None), BindingKey::of::<A>(None)]
    ));
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
fn test_graph_missing_sibling_is_reported_before_descendant_failure() {
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
    assert!(matches!(
        result,
        Err(BuildError::MissingDependency { path, definition, .. })
            if definition.item == "A" && path == vec![BindingKey::of::<A>(None)]
    ));
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
    assert!(
        matches!(result, Err(BuildError::DependencyCycle { path }) if path == vec![
            BindingKey::of::<A>(None),
            BindingKey::of::<String>(Some(BindingId::parse("first").expect("valid ID"))),
            BindingKey::of::<String>(Some(BindingId::parse("second").expect("valid ID"))),
            BindingKey::of::<String>(Some(BindingId::parse("first").expect("valid ID"))),
        ])
    );
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
fn test_all_active_reports_missing_alias_outside_root_closure() {
    let mut unused = instance("unused", A, vec![], None, false, 0, None);
    unused.add_alias(PendingBinding::alias::<String, String, _>(
        BindingKey::of::<String>(Some(BindingId::parse("alias").expect("valid ID"))),
        BindingKey::of::<String>(Some(BindingId::parse("missing").expect("valid ID"))),
        false,
        0,
        |value| value,
    ));
    let root = [Dependency::of::<B>()];
    let result = ValidatedGraph::validate_roots_with_scope(
        vec![instance("root", B, vec![], None, false, 0, None), unused],
        &[],
        Some(&root),
        ValidationScope::AllActive,
    );
    assert!(matches!(result, Err(BuildError::MissingAliasTarget { definition, .. }) if definition.item == "unused"));
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

#[test]
fn test_graph_root_exact_duplicates_are_ambiguous_in_registration_order() {
    let definitions = vec![
        instance("first", B, vec![], Some("exact"), false, 0, None),
        instance("second", B, vec![], Some("exact"), false, 0, None),
    ];
    let key = BindingKey::of::<B>(Some(BindingId::parse("exact").expect("valid ID")));
    let result = ValidatedGraph::validate_roots(definitions, &[], Some(&[Dependency::with_id::<B>("exact")]));
    assert!(
        matches!(result, Err(BuildError::AmbiguousRoot { candidates, .. }) if candidates == vec![key.clone(), key])
    );
}

#[test]
fn test_graph_root_ignores_unreachable_exact_and_primary_conflicts() {
    let graph = ValidatedGraph::validate_roots(
        vec![
            instance("root", A, vec![], None, false, 0, None),
            instance("first", B, vec![], Some("duplicate"), true, 0, None),
            instance("second", B, vec![], Some("duplicate"), true, 0, None),
        ],
        &[],
        Some(&[Dependency::of::<A>()]),
    )
    .expect("unreachable conflicts do not affect the root");
    assert_eq!(ordered_names(&graph), vec!["root"]);
}

#[test]
fn test_graph_root_reports_dependency_exact_ambiguity_with_full_path() {
    let exact = BindingKey::of::<C>(Some(BindingId::parse("exact").expect("valid ID")));
    let result = ValidatedGraph::validate_roots(
        vec![
            instance("A", A, vec![Dependency::of::<B>()], None, false, 0, None),
            instance("B", B, vec![Dependency::with_id::<C>("exact")], None, false, 0, None),
            instance("first", C, vec![], Some("exact"), false, 0, None),
            instance("second", C, vec![], Some("exact"), false, 0, None),
        ],
        &[],
        Some(&[Dependency::of::<A>()]),
    );
    assert!(
        matches!(result, Err(BuildError::AmbiguousBinding { candidates, path, definition, .. })
        if candidates == vec![exact.clone(), exact]
            && path == vec![BindingKey::of::<A>(None), BindingKey::of::<B>(None)]
            && definition.item == "B")
    );
}

#[test]
fn test_graph_missing_root_candidates_keep_registration_order() {
    let unnamed = BindingKey::of::<B>(None);
    let z = BindingKey::of::<B>(Some(BindingId::parse("z").expect("valid ID")));
    let a = BindingKey::of::<B>(Some(BindingId::parse("a").expect("valid ID")));
    let result = ValidatedGraph::validate_roots(
        vec![
            instance("z", B, vec![], Some("z"), false, 0, None),
            instance("unnamed", B, vec![], None, false, 0, None),
            instance("a", B, vec![], Some("a"), false, 0, None),
        ],
        &[],
        Some(&[Dependency::with_id::<B>("missing")]),
    );
    assert!(matches!(result, Err(BuildError::MissingRoot { available, .. }) if available == vec![z, unnamed, a]));
}

#[test]
fn test_graph_selected_duplicate_primaries_retain_source_order() {
    let result = ValidatedGraph::validate_roots(
        vec![
            instance("root", A, vec![Dependency::all::<B>()], None, false, 0, None),
            instance("z", B, vec![], Some("z"), true, 0, None),
            instance("a", B, vec![], Some("a"), true, 0, None),
        ],
        &[],
        Some(&[Dependency::of::<A>()]),
    );
    assert!(matches!(result, Err(BuildError::MultiplePrimaryBindings { candidates })
        if candidates.iter().map(|(_, source)| source.item).collect::<Vec<_>>() == vec!["z", "a"]));
}

#[test]
fn test_graph_alias_target_uses_first_duplicate_before_reachability_check() {
    let target = BindingKey::of::<B>(Some(BindingId::parse("target").expect("valid ID")));
    let mut root = instance("root", A, vec![], None, false, 0, None);
    root.add_alias(PendingBinding::alias::<B, B, _>(
        BindingKey::of::<B>(Some(BindingId::parse("alias").expect("valid ID"))),
        target,
        false,
        0,
        |value| value,
    ));
    let graph = ValidatedGraph::validate_roots(
        vec![
            root,
            instance("first", B, vec![], Some("target"), false, 0, None),
            instance("second", B, vec![Dependency::of::<C>()], Some("target"), false, 0, None),
        ],
        &[],
        Some(&[Dependency::of::<A>()]),
    )
    .expect("alias chooses first registration; second duplicate remains unreachable");
    assert_eq!(ordered_names(&graph), vec!["root", "first", "root"]);
}

#[test]
fn test_graph_alias_root_closes_all_members_and_dependencies() {
    const ALIAS_COUNT: usize = 10_000;
    let target = BindingKey::of::<u32>(None);
    let mut definition = instance(
        "aliases",
        42u32,
        vec![Dependency::optional_with_id::<B>("absent"), Dependency::all::<C>()],
        None,
        false,
        0,
        None,
    );
    for index in 0..ALIAS_COUNT {
        definition.add_alias(PendingBinding::alias::<u32, u32, _>(
            BindingKey::of::<u32>(Some(BindingId::parse(&format!("alias.n{index}")).expect("valid ID"))),
            target.clone(),
            false,
            0,
            |value| value,
        ));
    }
    let graph = ValidatedGraph::validate_roots(
        vec![definition],
        &[],
        Some(&[Dependency::with_id::<u32>("alias.n9999")]),
    )
    .expect("alias root selects every definition member");
    assert_eq!(graph.order.len(), ALIAS_COUNT + 1);
    assert!(graph.resolved[0].iter().all(|request| request.keys.is_empty()));
    assert_eq!(graph.order[0].binding, 0);
}

#[test]
fn test_graph_root_cycle_keeps_complete_root_prefix() {
    let result = ValidatedGraph::validate_roots(
        vec![
            instance("A", A, vec![Dependency::of::<B>()], None, false, 0, None),
            instance("B", B, vec![Dependency::of::<C>()], None, false, 0, None),
            instance("C", C, vec![Dependency::of::<B>()], None, false, 0, None),
        ],
        &[],
        Some(&[Dependency::of::<A>()]),
    );
    assert!(matches!(
        result,
        Err(BuildError::DependencyCycle { path })
            if path == vec![
                BindingKey::of::<A>(None),
                BindingKey::of::<B>(None),
                BindingKey::of::<C>(None),
                BindingKey::of::<B>(None),
            ]
    ));
}

#[test]
fn test_graph_alias_reachable_duplicate_target_is_rejected() {
    let mut root = instance("root", A, vec![Dependency::all::<B>()], None, false, 0, None);
    let target = BindingKey::of::<B>(Some(BindingId::parse("target").expect("valid ID")));
    root.add_alias(PendingBinding::alias::<B, B, _>(
        BindingKey::of::<B>(Some(BindingId::parse("alias").expect("valid ID"))),
        target.clone(),
        false,
        0,
        |value| value,
    ));
    let result = ValidatedGraph::validate_roots(
        vec![
            root,
            instance("first", B, vec![], Some("target"), false, 0, None),
            instance("second", B, vec![], Some("target"), false, 0, None),
        ],
        &[],
        Some(&[Dependency::of::<A>()]),
    );
    assert!(matches!(
        result,
        Err(BuildError::DuplicateBinding { key, first, second })
            if key == target && first.item == "first" && second.item == "second"
    ));
}

#[test]
fn test_graph_alias_root_still_checks_concrete_member_dependency_edges() {
    let mut definition = instance("member", A, vec![Dependency::of::<B>()], None, false, 0, None);
    let alias = BindingKey::of::<A>(Some(BindingId::parse("alias").expect("valid ID")));
    definition.add_alias(PendingBinding::alias::<A, A, _>(
        alias.clone(),
        BindingKey::of::<A>(None),
        false,
        0,
        |value| value,
    ));
    let result = ValidatedGraph::validate_roots(vec![definition], &[], Some(&[Dependency::with_id::<A>("alias")]));
    assert!(matches!(
        result,
        Err(BuildError::MissingDependency { path, definition, .. })
            if path == vec![alias, BindingKey::of::<A>(None)] && definition.item == "member"
    ));
}

#[test]
fn test_graph_build_all_large_alias_definition_keeps_member_dependency_closure() {
    const ALIAS_COUNT: usize = 10_000;
    let target = BindingKey::of::<A>(None);
    let mut definition = instance("members", A, vec![Dependency::of::<B>()], None, false, 0, None);
    for index in 0..ALIAS_COUNT {
        definition.add_alias(PendingBinding::alias::<A, A, _>(
            BindingKey::of::<A>(Some(BindingId::parse(&format!("alias.n{index}")).expect("valid ID"))),
            target.clone(),
            false,
            0,
            |value| value,
        ));
    }
    let graph = ValidatedGraph::validate(
        vec![
            definition,
            instance("dependency", B, vec![Dependency::of::<C>()], None, false, 0, None),
            instance("transitive", C, vec![], None, false, 0, None),
        ],
        &[],
    )
    .expect("all alias members retain concrete and transitive dependency edges");
    assert_eq!(graph.order.len(), ALIAS_COUNT + 3);
    assert_eq!(&ordered_names(&graph)[..3], &["transitive", "dependency", "members"]);
    assert_eq!(graph.resolved[0][0].keys, vec![BindingKey::of::<B>(None)]);
    assert_eq!(graph.resolved[1][0].keys, vec![BindingKey::of::<C>(None)]);
}
