//! Extended expression evaluation. `real` marks Pascal Real assignments or
//! by-value Real argument stores; intrinsic functions retain Extended operands.
use crate::{CoreError, Extended80, NumericError, Real48};
use std::ops::{Add, Div, Mul, Neg, Sub};
#[derive(Clone, Copy)]
pub(crate) struct E(pub Result<Extended80, NumericError>);
pub(crate) fn r(v: Real48) -> E {
    E(Ok(v.load_extended()))
}
pub(crate) fn c(v: Extended80) -> E {
    E(Ok(v))
}
pub(crate) fn i(v: i64) -> E {
    c(Extended80::from_i64(v))
}
impl E {
    pub fn real(self) -> Result<Real48, CoreError> {
        Ok(self.0?.store_real()?)
    }
    pub fn abs(self) -> Self {
        Self(self.0.map(Extended80::abs))
    }
    pub fn sqr(self) -> Self {
        self * self
    }
    pub fn sqrt(self) -> Self {
        Self(self.0.and_then(Extended80::checked_sqrt))
    }
    pub fn sin(self) -> Self {
        Self(self.0.and_then(diada_compat::sin))
    }
    pub fn cos(self) -> Self {
        Self(self.0.and_then(diada_compat::cos))
    }
    pub fn atan(self) -> Self {
        Self(self.0.and_then(diada_compat::atan))
    }
    pub fn cmp(self, other: Self) -> Result<std::cmp::Ordering, CoreError> {
        Ok(self.0?.compare(other.0?)?)
    }
    pub fn lt(self, other: Self) -> Result<bool, CoreError> {
        Ok(self.cmp(other)?.is_lt())
    }
    pub fn gt(self, other: Self) -> Result<bool, CoreError> {
        Ok(self.cmp(other)?.is_gt())
    }
    pub fn ge(self, other: Self) -> Result<bool, CoreError> {
        Ok(!self.lt(other)?)
    }
    pub fn le(self, other: Self) -> Result<bool, CoreError> {
        Ok(!self.gt(other)?)
    }
}
macro_rules! op {
    ($trait:ident,$method:ident,$checked:ident) => {
        impl $trait for E {
            type Output = Self;
            fn $method(self, rhs: Self) -> Self {
                Self(self.0.and_then(|a| rhs.0.and_then(|b| a.$checked(b))))
            }
        }
    };
}
op!(Add, add, checked_add);
op!(Sub, sub, checked_sub);
op!(Mul, mul, checked_mul);
op!(Div, div, checked_div);
impl Neg for E {
    type Output = Self;
    fn neg(self) -> Self {
        Self(self.0.map(Extended80::neg))
    }
}
