// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Linked provider definitions for cross-crate discovery.

mod app_service;
mod disk_repository;
mod greeting;
mod internal;
mod memory_repository;
mod preview_marker;
mod settings;

pub use app_service::AppService;
pub use disk_repository::DiskRepository;
pub use greeting::Greeting;
pub use memory_repository::MemoryRepository;
pub use preview_marker::PreviewMarker;
pub use settings::Settings;

pub use internal::linked_marker;
