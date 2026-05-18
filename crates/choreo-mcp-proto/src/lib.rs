//! Vendored Choreographer gRPC types for the standalone
//! `choreo-mcp` distribution. The internal workspace continues
//! to use `choreo-proto`; this crate exists so `choreo-mcp` can
//! be published to crates.io with all its proto deps already on
//! the registry.
//!
//! Wire contract: `underpass.choreo.v1`.

#![allow(clippy::pedantic)]
#![allow(clippy::all)]

pub mod v1 {
    tonic::include_proto!("underpass.choreo.v1");
}

pub use v1::*;
