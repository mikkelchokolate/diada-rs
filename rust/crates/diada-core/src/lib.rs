//! DIADA 3.20 mechanism model and calculation rules.
//! Numeric values are original Turbo Pascal Reals; display conversion is explicit.
mod expression;
mod force_traversal;
pub mod geometry;
mod graphs;
mod kinematics;
mod model;
mod sampling;
pub mod statics;
mod structure;
pub use diada_compat::{Extended80, NumericError, Real48};
pub use graphs::{GraphCategory, GraphObject, GraphValues};
pub use model::*;
pub use sampling::KinematicSampler;
