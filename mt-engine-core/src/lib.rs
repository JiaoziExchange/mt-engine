//! MT-Engine Core
//! High-performance, deterministic matching engine core implementation

#[cfg(all(feature = "snapshot", feature = "dense-node"))]
compile_error!("'snapshot' (exporting) and 'dense-node' features are mutually exclusive to ensure zero-cost on dense nodes. Use 'serde' feature for loading support on dense nodes.");

#[cfg(feature = "snapshot")]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

pub mod book;
pub mod codec;
pub mod command;
pub mod engine;
pub mod orders;
pub mod outcome;
pub mod snapshot;
pub mod types;

pub mod prelude {
    pub use crate::book::backend::OrderBookBackend;
    pub use crate::codec::CommandCodec;
    pub use crate::engine::events::OrderEventListener;
    pub use crate::engine::sbe_listener::SbeEncoderListener;
    pub use crate::engine::Engine;
    pub use crate::orders::*;
    pub use crate::outcome::*;
    pub use crate::types::*;
}

#[cfg(test)]
mod tests;
