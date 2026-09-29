// SPDX-License-Identifier: GPL-3.0-or-later

//! Declarative provider definitions: a site described in YAML, run by a
//! generic engine. Definitions are data, never code, so importing one from
//! a URL is safe (ADR 0005).

pub mod cardigann;
pub mod compile;
pub mod extract;
pub mod filters;
pub mod spec;
pub mod store;
pub mod templates;
pub mod xmlpath;

pub use compile::{CompiledDefinition, DefinitionError, compile, load, parse_yaml};
pub use spec::Definition;

#[cfg(test)]
mod tests;
