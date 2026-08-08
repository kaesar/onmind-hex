//! Hexagonal ABCode scripting service — the domain, pure.

pub mod domain;
pub mod application;
pub mod infrastructure;
pub mod graph;

#[cfg(feature = "graphql")]
pub mod graphql;

#[cfg(feature = "grpc")]
pub mod grpc;