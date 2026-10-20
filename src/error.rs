// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Structured error type shared by every NEXUS module.

use std::fmt;

/// Every failure NEXUS can report.
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// A field specification value violated a requirement.
    Spec {
        /// Name of the offending field.
        name: &'static str,
        /// What the field must satisfy.
        requirement: &'static str,
    },
    /// An index was outside its exclusive bound.
    Ordinal {
        /// What kind of index was supplied.
        kind: &'static str,
        /// The supplied value.
        value: u64,
        /// The exclusive upper bound.
        bound: u64,
    },
    /// A progress value was non-finite or outside `[0, 1]`.
    Progress(f64),
    /// A frequency was non-finite, non-positive, or overflowed.
    Frequency(f64),
    /// A count that must be positive was zero.
    Zero(&'static str),
    /// A lattice request was malformed.
    Lattice(&'static str),
    /// A worker thread panicked or could not be joined.
    Worker,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spec { name, requirement } => write!(f, "{name} {requirement}"),
            Self::Ordinal { kind, value, bound } => {
                write!(f, "{kind} index {value} is not below bound {bound}")
            }
            Self::Progress(value) => write!(f, "progress {value} is not a finite value in [0, 1]"),
            Self::Frequency(value) => {
                write!(f, "frequency {value} Hz is not a finite positive value")
            }
            Self::Zero(what) => write!(f, "{what} must be greater than zero"),
            Self::Lattice(reason) => write!(f, "lattice request rejected: {reason}"),
            Self::Worker => write!(f, "a worker thread failed"),
        }
    }
}

impl std::error::Error for Error {}

/// Result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;
