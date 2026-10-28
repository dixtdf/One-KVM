// SPDX-License-Identifier: AGPL-3.0-only
pub mod model;
pub mod discovery;
pub mod migration;
pub mod runtime;
pub mod store;
mod web;
pub use web::{router, Service};
