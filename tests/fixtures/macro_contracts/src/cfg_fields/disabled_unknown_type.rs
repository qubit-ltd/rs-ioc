// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Component whose disabled field names a type alias that must never be
//! resolved.

use r#type::Component;

/// Alias named only by a permanently disabled field.
///
/// The alias exists so the unsupported field type has a name that the
/// `#[Component]` expansion must never resolve: a component field whose type
/// the macro rejects is only tolerated when the field is stripped first, so
/// this alias never reaches type resolution in any supported feature
/// combination. Keeping the alias private to this module is what guarantees
/// the fixture cannot accidentally acquire a second, resolvable use site.
type MissingType = u32;

/// Contains an unsupported field type behind a false condition.
#[Component]
pub struct DisabledUnknownType {
    /// Field whose type the macro must skip instead of rejecting.
    ///
    /// `MissingType` stands for a field type `#[Component]` does not support.
    /// Because the field is gated by the always-false `cfg(any())`, the
    /// expansion has to drop the field before inspecting its type, so no
    /// diagnostic and no generated binding for `MissingType` may appear. A
    /// consumer that ever sees this alias in expanded output, or in a compile
    /// error, has broken that contract.
    #[cfg(any())]
    missing: MissingType,
}
