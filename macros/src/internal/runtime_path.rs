// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Resolves the exact runtime dependency path used by a consumer.

use proc_macro_crate::FoundCrate;
use proc_macro_crate::crate_name;
use proc_macro2::Span;
use proc_macro2::TokenStream;
use quote::quote;
use syn::AngleBracketedGenericArguments;
use syn::Error;
use syn::GenericArgument;
use syn::Ident;
use syn::PathArguments;
use syn::Result;
use syn::Type;
use syn::ext::IdentExt;

/// Runtime crate identity resolved from the consuming package manifest.
pub(crate) struct RuntimePath {
    /// Root identifier of the runtime crate dependency.
    root: Ident,
}

impl RuntimePath {
    /// Creates a path resolver for parser and validator unit tests.
    #[cfg(test)]
    pub(crate) fn for_root(root: &str) -> Self {
        Self {
            root: Ident::new_raw(root, Span::call_site()),
        }
    }

    /// Resolves the runtime crate name from the consumer's Cargo manifest.
    pub(crate) fn resolve() -> Result<Self> {
        match crate_name("qubit-ioc") {
            Ok(FoundCrate::Itself) => Ok(Self {
                root: Ident::new("qubit_ioc", Span::call_site()),
            }),
            Ok(FoundCrate::Name(name)) => Ok(Self {
                root: Ident::new_raw(&name, Span::call_site()),
            }),
            Err(error) => Err(Error::new(
                Span::call_site(),
                format!("cannot locate `qubit-ioc` runtime dependency: {error}"),
            )),
        }
    }

    /// Returns the absolute token path used by generated code.
    #[must_use]
    pub(crate) fn tokens(&self) -> TokenStream {
        let root = &self.root;
        quote!(::#root)
    }

    /// Returns the inner type for a precisely qualified runtime `Managed<T>`.
    ///
    /// Bare `Managed<T>` is accepted because it may be explicitly imported.
    /// Unrelated qualified paths are left as ordinary component types.
    #[must_use]
    pub(crate) fn managed_argument<'a>(&self, ty: &'a Type) -> Option<&'a Type> {
        let Type::Path(type_path) = ty else { return None };
        if type_path.qself.is_some() {
            return None;
        }
        let path = &type_path.path;
        let segments = path.segments.iter().collect::<Vec<_>>();
        let managed = match segments.as_slice() {
            [managed] if path.leading_colon.is_none() && managed.ident == "Managed" => managed,
            [root, managed] if root.ident.unraw() == self.root.unraw() && managed.ident == "Managed" => {
                if !matches!(root.arguments, PathArguments::None) {
                    return None;
                }
                managed
            }
            _ => return None,
        };
        let PathArguments::AngleBracketed(AngleBracketedGenericArguments { args, .. }) = &managed.arguments else {
            return None;
        };
        if args.len() != 1 {
            return None;
        }
        let GenericArgument::Type(inner) = args.first()? else {
            return None;
        };
        Some(inner)
    }
}

#[cfg(test)]
mod tests {
    use proc_macro2::Span;
    use syn::Ident;
    use syn::Type;
    use syn::parse_str;

    use super::RuntimePath;

    /// Creates a resolver for an explicitly selected consumer alias.
    fn runtime(root: &str) -> RuntimePath {
        RuntimePath {
            root: Ident::new_raw(root, Span::call_site()),
        }
    }

    /// Accepts only bare or exact runtime-qualified managed paths.
    #[test]
    fn recognizes_exact_managed_paths() {
        let runtime = runtime("ioc");
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
}
