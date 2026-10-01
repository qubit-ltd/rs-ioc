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
    ///
    /// The identifier is always built in raw form, so a dependency renamed to a
    /// Rust keyword such as `type` stays addressable. Every emitted path places
    /// it behind a leading `::`, which keeps generated code independent of the
    /// consumer's `use` declarations and of any same-named local item.
    root: Ident,
}

impl RuntimePath {
    /// Creates a path resolver for parser and validator unit tests.
    ///
    /// This is the seam that lets crate-level tests drive the expansion and
    /// validation code against a chosen consumer alias. It bypasses
    /// [`Self::resolve`] entirely: no manifest is read and no IO happens, so a
    /// test can name an alias that no real manifest would contain.
    ///
    /// # Parameters
    ///
    /// `root` is taken verbatim as the runtime crate alias and is turned into a
    /// raw identifier at the call site, exactly as [`Self::resolve`] would. The
    /// caller must pass a valid Rust identifier; a string that is not one is
    /// not rejected here but produces tokens that will not resolve.
    ///
    /// # Returns
    ///
    /// A resolver whose only state is that alias, so [`Self::tokens`] and
    /// [`Self::managed_argument`] behave as they would for a consumer that
    /// renamed the runtime dependency to `root`.
    #[cfg(test)]
    pub(crate) fn for_root(root: &str) -> Self {
        Self {
            root: Ident::new_raw(root, Span::call_site()),
        }
    }

    /// Resolves the runtime crate name from the consumer's Cargo manifest.
    ///
    /// `FoundCrate::Itself` means the macro expanded inside the runtime crate
    /// itself and yields the plain `qubit_ioc` identifier; a renamed dependency
    /// keeps the alias spelled in the manifest. The lookup reads only the
    /// consumer manifest and performs no other IO.
    ///
    /// # Errors
    ///
    /// Returns a [`syn::Error`] anchored at [`Span::call_site`] when the
    /// consuming package declares no `qubit-ioc` dependency, or when
    /// `proc-macro-crate` cannot read its manifest at all.
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
    ///
    /// # Returns
    ///
    /// A freshly built [`TokenStream`] spelling `::` followed by the raw root
    /// identifier. Each call allocates a new stream and performs no IO.
    #[must_use]
    pub(crate) fn tokens(&self) -> TokenStream {
        let root = &self.root;
        quote!(::#root)
    }

    /// Returns the inner type for a precisely qualified runtime `Managed<T>`.
    ///
    /// Bare `Managed<T>` is accepted because it may be explicitly imported.
    /// Unrelated qualified paths are left as ordinary component types.
    ///
    /// # Parameters
    ///
    /// `ty` is the component field or factory return type to inspect. The
    /// borrow is only read and never retained.
    ///
    /// # Returns
    ///
    /// `Some(inner)` when `ty` is the single type argument of a bare
    /// `Managed<T>` or of `root::Managed<T>` whose root matches this runtime
    /// and carries no generic arguments of its own.
    ///
    /// `None` for every other spelling, namely: a type that is not a path; a
    /// qualified path such as `<T as Trait>::Assoc`; a bare path that is not
    /// `Managed` or that is written with a leading `::` at the top level; a
    /// leading segment other than this runtime; a path with a segment count
    /// other than one or two; a root segment that carries generic arguments;
    /// a `Managed` segment whose arguments are not angle-bracketed; a generic
    /// argument list whose length is not one; and a single argument that is not
    /// itself a type.
    #[must_use]
    pub(crate) fn managed_argument<'a>(&self, ty: &'a Type) -> Option<&'a Type> {
        let Type::Path(type_path) = ty else {
            return None;
        };
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
