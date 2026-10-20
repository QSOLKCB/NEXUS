// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! # NEXUS
//!
//! A deterministic engine for *centre-transit fields*: a logical field of
//! arbitrary cardinality is represented by a bounded set of strands, each of
//! which travels from an ingress point through the exact origin (the nexus)
//! to its antipodal egress. Around that geometry NEXUS provides:
//!
//! - exact integer strand selection (uniform floor partition, or a 64-bit
//!   fixed-point golden ordering) with no allocation proportional to the
//!   logical cardinality;
//! - a branch-free signed-progress transit formula whose centre is literally
//!   zero and whose endpoints are antipodal by construction;
//! - ternary metadata: a transit [`Trit`] and a deterministic execution
//!   [`Lane`];
//! - sequential and order-preserving parallel evaluation on the standard
//!   library only;
//! - lane-separated SHA-256 [`Ledger`]s over a fixed canonical encoding;
//! - deterministic sonification controls;
//! - an invariant [`Audit`] that is identical on one thread or many;
//! - cuboctahedral engines: a twelve-direction state atlas and an exactly
//!   reversible coupled `Z_4` lattice with a CPU reference and an optional
//!   Vulkan/WGSL mirror behind the `gpu` feature.
//!
//! Everything is classical and deterministic. Trits and lanes are labels
//! over continuous geometry, ledgers prove byte identity, and the lattice is
//! a reversible state machine. None of it asserts physics.
//!
//! ```
//! use nexus::{FieldSpec, Job, Ordering, Pos};
//!
//! let field = FieldSpec::default().build()?;
//! let sample = nexus::sample(&field, Ordering::Uniform, Job { ordinal: 17, progress: 0.5 })?;
//! assert_eq!(sample.pos, Pos::ORIGIN);
//! assert_eq!(sample.trit, nexus::Trit::Nexus);
//! # Ok::<(), nexus::Error>(())
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod audit;
pub mod error;
pub mod exec;
pub mod field;
#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
pub mod gpu;
pub mod ledger;
pub mod sha256;
pub mod sonify;
pub mod ternary;
pub mod transit;
pub mod ve;

pub use audit::{audit, audit_parallel, Audit};
pub use error::{Error, Result};
pub use exec::{
    canonical_job, effective_workers, run, run_parallel, sample, sample_strand, Job, Sample, StrandTable, MAX_WORKERS,
};
pub use field::{golden_logical, uniform_logical, Field, FieldSpec, Ordering, Strand, StrandIter};
pub use ledger::{canonical_ledger, encode, ledger, Ledger, LedgerBuilder, RECORD_BYTES};
pub use sha256::{hex, Digest, Sha256};
pub use sonify::{event, Event, EventMap};
pub use ternary::{Lane, Trit};
pub use transit::{eversion, position, position_signed, signed, Pos};
