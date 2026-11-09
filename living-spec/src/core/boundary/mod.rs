//! Core-owned boundary annotation models and coverage projection.
pub mod coverage;
mod gen;
pub mod model;
pub use coverage::{
    coverage, BoundCoverage, CaseLink, CaseStatus, CoverageMap, CoveredCase, GapReason,
};
pub use gen::{generate, Example, Expected};
pub use model::{Bound, BoundKind, BoundaryError, Decimal, Op, Step, Unit};
