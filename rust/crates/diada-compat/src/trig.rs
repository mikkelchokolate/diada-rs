use crate::arithmetic::{finite, pack};
use crate::constants::{HALF_PI, QUARTER_PI};
use crate::{Extended80, NumericError};
use std::cmp::Ordering;

fn tangent(value: Extended80) -> Result<Extended80, NumericError> {
    let f = finite(value)?;
    if f.significand == 0 || f.exponent < -64 {
        return Ok(value);
    }
    crate::native_trig::tangent(value)
}

fn remainder(value: Extended80) -> Result<(Extended80, u8), NumericError> {
    let a = finite(value.abs())?;
    let b = finite(QUARTER_PI)?;
    let shift = a.exponent - b.exponent;
    if shift < 0 {
        return Ok((value.abs(), 0));
    }
    if shift >= 64 {
        return Err(NumericError::InvalidOperation);
    }
    let n = u128::from(a.significand) << shift;
    let divisor = u128::from(b.significand);
    Ok((
        pack(n % divisor, b.exponent - 63, false)?,
        ((n / divisor) & 7) as u8,
    ))
}

fn sin_cos(value: Extended80, cosine: bool) -> Result<Extended80, NumericError> {
    let a = finite(value)?;
    if a.significand == 0 {
        return Ok(if cosine {
            Extended80::from_i64(1)
        } else {
            value
        });
    }
    let (rem, q) = remainder(value)?;
    let angle = if q & 1 != 0 {
        QUARTER_PI.checked_sub(rem)?
    } else {
        rem
    };
    let tan = tangent(angle)?;
    let square = tan.checked_mul(tan)?;
    let denominator = square
        .checked_add(Extended80::from_i64(1))?
        .checked_sqrt()?;
    let use_tan = if cosine {
        matches!(q & 3, 1 | 2)
    } else {
        matches!(q & 3, 0 | 3)
    };
    let numerator = if use_tan {
        tan
    } else {
        Extended80::from_i64(1)
    };
    let negative = if cosine { matches!(q, 2..=5) } else { q >= 4 } ^ (!cosine && a.negative);
    let numerator = if negative { numerator.neg() } else { numerator };
    numerator.checked_div(denominator)
}

pub fn sin(value: Extended80) -> Result<Extended80, NumericError> {
    sin_cos(value, false)
}
pub fn cos(value: Extended80) -> Result<Extended80, NumericError> {
    sin_cos(value, true)
}
pub fn atan(value: Extended80) -> Result<Extended80, NumericError> {
    let f = finite(value)?;
    if f.significand == 0 || f.exponent < -64 {
        return Ok(value);
    }
    let one = Extended80::from_i64(1);
    let order = value.abs().compare(one)?;
    let mut result = if order == Ordering::Equal {
        QUARTER_PI
    } else {
        let small = if order == Ordering::Greater {
            crate::native_trig::atan_ratio(one, value.abs())?
        } else {
            crate::native_trig::atan_ratio(value.abs(), one)?
        };
        if order == Ordering::Greater {
            small.checked_sub(HALF_PI)?.neg()
        } else {
            small
        }
    };
    if f.negative {
        result = result.neg();
    }
    Ok(result)
}
