// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Component, field, and dependency validation.

use syn::Attribute;
use syn::Error;
use syn::Fields;
use syn::GenericArgument;
use syn::ItemStruct;
use syn::LitStr;
use syn::Meta;
use syn::PathArguments;
use syn::Result;
use syn::Type;
use syn::spanned::Spanned;

use super::ValidatedOptions;
use super::options::validate_id;
use crate::conditions::activation_attributes;
use crate::ir::ComponentIr;
use crate::ir::DependencyIr;
use crate::ir::DependencyKind;
use crate::ir::FieldIr;
use crate::ir::MacroKind;
use crate::ir::SourceIr;
use crate::parse::RawOption;
use crate::parse::RawValue;
use crate::parse::parse_options;

/// Normalizes a named-field or unit struct into ordered dependency requests.
pub(super) fn component(kind: MacroKind, mut item: ItemStruct, options: ValidatedOptions) -> Result<ComponentIr> {
    validate_struct(&item, kind)?;
    let source = SourceIr {
        item: item.ident.clone(),
        span: item.ident.span(),
    };
    let mut fields = Vec::new();
    if let Fields::Named(named) = &mut item.fields {
        fields.reserve(named.named.len());
        for field in &mut named.named {
            let ident = field.ident.clone().expect("named fields always have identifiers");
            let conditions = activation_attributes(&field.attrs)?;
            let dependency = dependency(&mut field.attrs, &field.ty)?;
            fields.push(FieldIr {
                ident,
                dependency,
                conditions,
            });
        }
    }
    Ok(ComponentIr {
        item,
        options: options.binding(),
        fields,
        source,
    })
}

/// Validates that a struct has a supported shape and no generic parameters.
pub(super) fn validate_struct(item: &ItemStruct, kind: MacroKind) -> Result<()> {
    if !item.generics.params.is_empty() || item.generics.where_clause.is_some() {
        return Err(Error::new(
            item.generics.span(),
            format!("#[{}] does not support generic structs", kind.name()),
        ));
    }
    if matches!(item.fields, Fields::Unnamed(_)) {
        return Err(Error::new(
            item.fields.span(),
            format!("#[{}] requires named fields or a unit struct", kind.name()),
        ));
    }
    Ok(())
}

/// Converts field and parameter helper attributes into a typed dependency.
pub(super) fn dependency(attributes: &mut Vec<Attribute>, ty: &Type) -> Result<DependencyIr> {
    let mut id = None;
    let mut value_path = None;
    let mut seen_inject = false;
    let mut seen_value = false;
    let mut helper_span = ty.span();
    let mut retained = Vec::with_capacity(attributes.len());
    for attribute in attributes.drain(..) {
        if attribute.path().is_ident("inject") {
            if seen_inject || seen_value {
                return Err(Error::new(
                    attribute.span(),
                    "duplicate or conflicting `inject` attribute",
                ));
            }
            seen_inject = true;
            helper_span = attribute.span();
            let options = match &attribute.meta {
                Meta::Path(_) => Vec::new(),
                Meta::List(_) => parse_options(attribute.parse_args()?)?,
                Meta::NameValue(_) => {
                    return Err(Error::new(
                        attribute.span(),
                        "`inject` accepts a bare marker or `#[inject(id = \"...\")]`",
                    ));
                }
            };
            for RawOption { key, value } in options {
                if key != "id" {
                    return Err(Error::new(
                        key.span(),
                        "`inject` only accepts `id`, not `name` or other options",
                    ));
                }
                if id.is_some() {
                    return Err(Error::new(key.span(), "duplicate `inject` option `id`"));
                }
                let RawValue::String(value) = value else {
                    return Err(Error::new(key.span(), "`inject(id = ...)` requires a string literal"));
                };
                validate_id(&value)?;
                id = Some(value);
            }
        } else if attribute.path().is_ident("value") {
            if seen_inject || seen_value {
                return Err(Error::new(
                    attribute.span(),
                    "`value` cannot be combined with `inject` or another `value`",
                ));
            }
            seen_value = true;
            helper_span = attribute.span();
            value_path = Some(attribute.parse_args::<LitStr>()?);
        } else {
            retained.push(attribute);
        }
    }
    *attributes = retained;
    if let Some(path) = value_path {
        return Ok(DependencyIr {
            kind: DependencyKind::Value { path },
            requested_type: ty.clone(),
            id: None,
        });
    }
    let (kind, requested_type) = classify_dependency(ty)?;
    if matches!(kind, DependencyKind::All) && id.is_some() {
        return Err(Error::new(
            helper_span,
            "`#[inject(id = ...)]` is unsupported on `Vec<Arc<T>>`",
        ));
    }
    Ok(DependencyIr {
        kind,
        requested_type,
        id,
    })
}

/// Recognizes exactly `Arc<T>`, `Option<Arc<T>>`, and `Vec<Arc<T>>`.
fn classify_dependency(ty: &Type) -> Result<(DependencyKind, Type)> {
    if let Some(inner) = single_generic(ty, "Arc") {
        return Ok((DependencyKind::Required, inner.clone()));
    }
    if let Some(inner) = single_generic(ty, "Option")
        && let Some(target) = single_generic(inner, "Arc")
    {
        return Ok((DependencyKind::Optional, target.clone()));
    }
    if let Some(inner) = single_generic(ty, "Vec")
        && let Some(target) = single_generic(inner, "Arc")
    {
        return Ok((DependencyKind::All, target.clone()));
    }
    Err(Error::new(
        ty.span(),
        "unsupported injection type; expected `Arc<T>`, `Option<Arc<T>>`, or `Vec<Arc<T>>`",
    ))
}

/// Returns the single type argument of a supported standard generic path.
pub(super) fn single_generic<'a>(ty: &'a Type, name: &str) -> Option<&'a Type> {
    let Type::Path(path) = ty else { return None };
    if path.qself.is_some() {
        return None;
    }

    let segments = path
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();
    let supported_paths: &[&[&str]] = match name {
        "Arc" => &[&["Arc"], &["std", "sync", "Arc"], &["alloc", "sync", "Arc"]],
        "Option" => &[&["Option"], &["std", "option", "Option"], &["core", "option", "Option"]],
        "Vec" => &[&["Vec"], &["std", "vec", "Vec"], &["alloc", "vec", "Vec"]],
        _ => &[],
    };
    if !supported_paths
        .iter()
        .any(|candidate| segments.iter().map(String::as_str).eq(candidate.iter().copied()))
    {
        return None;
    }
    let segment = path.path.segments.last()?;
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    if args.args.len() != 1 {
        return None;
    }
    let GenericArgument::Type(inner) = args.args.first()? else {
        return None;
    };
    Some(inner)
}
