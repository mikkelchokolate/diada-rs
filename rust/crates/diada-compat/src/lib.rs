//! Numeric and text compatibility with the original DIADA 3.20 runtime.
//!
//! Calculations use integer arithmetic with explicit Extended precision and
//! Real48 stores. The selected oracle is ordinary DOSBox AUTO with CW 1332.
//! Trigonometry reproduces the original TP reduction and Intel x87 internal
//! arithmetic model. Legacy Val/Str parsing and formatting retain TP semantics.

mod arithmetic;
pub mod constants;
mod fixed;
mod native_trig;
mod native_trig_constants;
mod numbers;
mod text_numbers;
mod trig;

pub use arithmetic::multiply_real48;
pub use numbers::{Extended80, NumericError, Real48};
pub use text_numbers::{ValError, format_extended, format_real, parse_real};
pub use trig::{atan, cos, sin};
