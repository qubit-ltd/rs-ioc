// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Local semantic checks and normalization into expander-ready IoC IR.

use std::collections::HashSet;

use syn::Attribute;
use syn::Error;
use syn::Fields;
use syn::FnArg;
use syn::GenericArgument;
use syn::Item;
use syn::ItemFn;
use syn::ItemMod;
use syn::ItemStruct;
use syn::LitStr;
use syn::Meta;
use syn::Pat;
use syn::PathArguments;
use syn::Result;
use syn::ReturnType;
use syn::Type;
use syn::TypeParamBound;
use syn::spanned::Spanned;

use crate::conditions::activation_attributes;
use crate::ir::BeanIr;
use crate::ir::ComponentIr;
use crate::ir::ConfigurationIr;
use crate::ir::ConfigurationPropertiesIr;
use crate::ir::Declaration;
use crate::ir::DependencyIr;
use crate::ir::DependencyKind;
use crate::ir::FieldIr;
use crate::ir::MacroKind;
use crate::ir::OutputIr;
use crate::ir::OutputShape;
use crate::ir::ParamIr;
use crate::ir::SourceIr;
use crate::parse::RawDeclaration;
use crate::parse::RawOption;
use crate::parse::RawValue;
use crate::parse::missing_option;
use crate::parse::parse_options;
use validated_options::ValidatedOptions;

mod validated_options;

/// Validates a parsed declaration and normalizes it for its later expander.
pub(crate) fn validate(raw: RawDeclaration) -> Result<Declaration> {
    let kind = raw.kind;
    let options = validate_options(kind, raw.options)?;
    match (kind, raw.item) {
        (MacroKind::Component | MacroKind::Service | MacroKind::Repository, Item::Struct(item)) => {
            component(kind, item, options).map(Declaration::Component)
        }
        (MacroKind::Bean, Item::Fn(item)) => bean(item, options).map(|value| Declaration::Bean(Box::new(value))),
        (MacroKind::Configuration, Item::Mod(item)) => configuration(item, options).map(Declaration::Configuration),
        (MacroKind::ConfigurationProperties, Item::Struct(item)) => {
            configuration_properties(item, options).map(Declaration::ConfigurationProperties)
        }
        (_, item) => Err(Error::new(
            item.span(),
            format!("#[{}] cannot be used on this Rust item", kind.name()),
        )),
    }
}

/// Checks option presence, duplicates, type restrictions, and ID literals.
fn validate_options(kind: MacroKind, options: Vec<RawOption>) -> Result<ValidatedOptions> {
    let mut seen = HashSet::new();
    let mut validated = ValidatedOptions::default();
    for RawOption { key, value } in options {
        let name = key.to_string();
        let allowed = match kind {
            MacroKind::Component | MacroKind::Service | MacroKind::Repository => {
                matches!(name.as_str(), "id" | "bind" | "primary" | "order" | "profile")
            }
            MacroKind::Bean => matches!(
                name.as_str(),
                "id" | "bind" | "primary" | "order" | "profile" | "type" | "marker"
            ),
            MacroKind::Configuration => name == "profile",
            MacroKind::ConfigurationProperties => {
                matches!(name.as_str(), "prefix" | "id" | "primary" | "order" | "profile")
            }
        };
        if !allowed {
            return Err(Error::new(
                key.span(),
                format!("unknown #[{}] option `{name}`", kind.name()),
            ));
        }
        if name != "bind" && !seen.insert(name.clone()) {
            return Err(Error::new(
                key.span(),
                format!("duplicate #[{}] option `{name}`", kind.name()),
            ));
        }
        match (name.as_str(), value) {
            ("id", RawValue::String(value)) => {
                validate_id(&value)?;
                validated.id = Some(value);
            }
            ("bind", RawValue::Type(Type::TraitObject(value))) => validated.binds.push(value),
            ("bind", _) => {
                return Err(Error::new(key.span(), "`bind` requires `dyn Trait`"));
            }
            ("primary", RawValue::Flag) => validated.primary = true,
            ("order", RawValue::Integer { literal, negative }) => {
                let magnitude = literal
                    .base10_parse::<i64>()
                    .map_err(|_| Error::new(literal.span(), "`order` must fit in a 32-bit integer"))?;
                let signed = if negative { -magnitude } else { magnitude };
                validated.order = i32::try_from(signed)
                    .map_err(|_| Error::new(literal.span(), "`order` must fit in a 32-bit integer"))?;
            }
            ("profile", RawValue::String(value)) => validated.profile = Some(value),
            ("prefix", RawValue::String(value)) => validated.prefix = Some(value),
            ("type", RawValue::Type(value)) => validated.explicit_type = Some(value),
            ("marker", RawValue::Ident(value)) => validated.marker = Some(value),
            _ => return Err(Error::new(key.span(), "invalid IoC option value")),
        }
    }
    Ok(validated)
}

/// Checks the same ASCII segmented ID grammar as the runtime `BindingId`.
fn validate_id(value: &LitStr) -> Result<()> {
    let text = value.value();
    let valid = !text.is_empty()
        && text.split('.').all(|segment| {
            let mut bytes = segment.bytes();
            bytes.next().is_some_and(|first| first.is_ascii_alphabetic())
                && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        });
    if valid {
        Ok(())
    } else {
        Err(Error::new(
            value.span(),
            "invalid `id`: each dot-separated segment must match [A-Za-z][A-Za-z0-9_]*",
        ))
    }
}

/// Normalizes a named-field or unit struct into ordered dependency requests.
fn component(kind: MacroKind, mut item: ItemStruct, options: ValidatedOptions) -> Result<ComponentIr> {
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
        kind,
        item,
        options: options.binding(),
        fields,
        source,
    })
}

/// Validates that a struct has a supported shape and no generic parameters.
fn validate_struct(item: &ItemStruct, kind: MacroKind) -> Result<()> {
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
fn dependency(attributes: &mut Vec<Attribute>, ty: &Type) -> Result<DependencyIr> {
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
            span: helper_span,
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
        span: helper_span,
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
fn single_generic<'a>(ty: &'a Type, name: &str) -> Option<&'a Type> {
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
        "Managed" => &[&["Managed"]],
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

/// Normalizes a bean function's parameters, output, and marker option.
fn bean(mut item: ItemFn, options: ValidatedOptions) -> Result<BeanIr> {
    let signature = &item.sig;
    if signature.constness.is_some()
        || signature.unsafety.is_some()
        || signature.abi.is_some()
        || signature.variadic.is_some()
        || !signature.generics.params.is_empty()
        || signature.generics.where_clause.is_some()
    {
        return Err(Error::new(
            signature.span(),
            "#[bean] requires a safe, non-generic Rust function",
        ));
    }
    let source = SourceIr {
        item: signature.ident.clone(),
        span: signature.ident.span(),
    };
    let output = output(&signature.output, options.explicit_type.as_ref())?;
    let mut params = Vec::with_capacity(signature.inputs.len());
    for argument in &mut item.sig.inputs {
        let FnArg::Typed(argument) = argument else {
            return Err(Error::new(
                argument.span(),
                "#[bean] cannot be used on methods or receivers",
            ));
        };
        let Pat::Ident(pattern) = argument.pat.as_ref() else {
            return Err(Error::new(
                argument.pat.span(),
                "#[bean] parameters must be named identifiers",
            ));
        };
        let ident = pattern.ident.clone();
        let dependency = dependency(&mut argument.attrs, &argument.ty)?;
        params.push(ParamIr { ident, dependency });
    }
    let marker = options.marker.clone();
    Ok(BeanIr {
        item,
        options: options.binding(),
        marker,
        params,
        output,
        source,
    })
}

/// Decomposes the supported bean output forms and an optional explicit type.
fn output(return_type: &ReturnType, explicit_type: Option<&Type>) -> Result<OutputIr> {
    let ReturnType::Type(_, ty) = return_type else {
        return Err(Error::new(return_type.span(), "#[bean] requires a return type"));
    };
    let (inner, error_type, result) = if let Some((success, error)) = result_types(ty) {
        (success, Some(error.clone()), true)
    } else {
        (ty.as_ref(), None, false)
    };
    if contains_future_type(inner) {
        return Err(Error::new(
            inner.span(),
            "#[bean] does not support a nested future output; use `async fn` instead",
        ));
    }
    if matches!(inner, Type::ImplTrait(_) | Type::Reference(_)) {
        return Err(Error::new(
            inner.span(),
            "#[bean] does not support `impl Trait` or borrowed outputs",
        ));
    }
    let (inner, managed) = if let Some(inner) = single_generic(inner, "Managed") {
        (inner, true)
    } else {
        (inner, false)
    };
    let (component_type, arc) = if !managed && let Some(inner) = single_generic(inner, "Arc") {
        (inner.clone(), true)
    } else {
        (inner.clone(), false)
    };
    let component_type = explicit_type.cloned().unwrap_or(component_type);
    if matches!(component_type, Type::ImplTrait(_) | Type::Reference(_)) {
        return Err(Error::new(
            component_type.span(),
            "#[bean] requires an owned concrete output type",
        ));
    }
    let shape = match (result, arc) {
        (false, false) => OutputShape::Bare,
        (false, true) => OutputShape::Arc,
        (true, false) => OutputShape::ResultBare,
        (true, true) => OutputShape::ResultArc,
    };
    Ok(OutputIr {
        shape,
        managed,
        component_type,
        error_type,
        span: return_type.span(),
    })
}

/// Extracts exactly two type arguments from `Result` or its standard qualified
/// paths.
fn result_types(ty: &Type) -> Option<(&Type, &Type)> {
    let Type::Path(path) = ty else { return None };
    if path.qself.is_some() {
        return None;
    }
    let segments = &path.path.segments;
    let standard = segments.len() == 3
        && matches!(segments[0].ident.to_string().as_str(), "std" | "core")
        && segments[1].ident == "result"
        && segments[2].ident == "Result";
    let plain = path.path.leading_colon.is_none() && segments.len() == 1 && segments[0].ident == "Result";
    if !plain && !standard {
        return None;
    }
    let segment = segments.last()?;
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    if args.args.len() != 2 {
        return None;
    }
    let mut args = args.args.iter();
    let GenericArgument::Type(success) = args.next()? else {
        return None;
    };
    let GenericArgument::Type(error) = args.next()? else {
        return None;
    };
    Some((success, error))
}

/// Detects syntactically visible futures inside a factory's returned value.
fn contains_future_type(ty: &Type) -> bool {
    match ty {
        Type::Path(path) => path.path.segments.iter().any(|segment| {
            segment.ident == "Future"
                || match &segment.arguments {
                    PathArguments::AngleBracketed(args) => args.args.iter().any(|arg| match arg {
                        GenericArgument::Type(inner) => contains_future_type(inner),
                        GenericArgument::AssocType(assoc) => contains_future_type(&assoc.ty),
                        _ => false,
                    }),
                    _ => false,
                }
        }),
        Type::TraitObject(object) => object.bounds.iter().any(|bound| match bound {
            TypeParamBound::Trait(trait_bound) => trait_bound
                .path
                .segments
                .iter()
                .any(|segment| segment.ident == "Future"),
            _ => false,
        }),
        Type::Group(group) => contains_future_type(&group.elem),
        Type::Paren(paren) => contains_future_type(&paren.elem),
        Type::Tuple(tuple) => tuple.elems.iter().any(contains_future_type),
        Type::Slice(slice) => contains_future_type(&slice.elem),
        Type::Array(array) => contains_future_type(&array.elem),
        _ => false,
    }
}

/// Normalizes an inline module and rejects a conflicting generated function.
fn configuration(item: ItemMod, options: ValidatedOptions) -> Result<ConfigurationIr> {
    let Some((_, items)) = &item.content else {
        return Err(Error::new(item.span(), "#[Configuration] requires an inline module"));
    };
    for child in items {
        if let Item::Fn(function) = child
            && function.sig.ident == "register_ioc"
        {
            return Err(Error::new(
                function.sig.ident.span(),
                "#[Configuration] conflicts with existing `register_ioc`",
            ));
        }
    }
    let source = SourceIr {
        item: item.ident.clone(),
        span: item.ident.span(),
    };
    Ok(ConfigurationIr {
        item,
        profile: options.profile,
        source,
    })
}

/// Normalizes a deserializable configuration struct with a required prefix.
fn configuration_properties(item: ItemStruct, mut options: ValidatedOptions) -> Result<ConfigurationPropertiesIr> {
    validate_struct(&item, MacroKind::ConfigurationProperties)?;
    if !matches!(item.fields, Fields::Named(_)) {
        return Err(Error::new(
            item.fields.span(),
            "#[ConfigurationProperties] requires named fields",
        ));
    }
    let prefix = options
        .prefix
        .take()
        .ok_or_else(|| missing_option(item.span(), "prefix"))?;
    let source = SourceIr {
        item: item.ident.clone(),
        span: item.ident.span(),
    };
    Ok(ConfigurationPropertiesIr {
        item,
        prefix,
        options: options.binding(),
        source,
    })
}
