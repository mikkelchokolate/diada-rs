use crate::{Extended80, NumericError, Real48};
use std::cmp::Ordering;

/// Multiply two loaded Turbo Pascal Reals with a 64-bit Extended significand.
///
/// The exact product of the two 40-bit significands fits in `u128`. It is
/// rounded once to 64 significant bits, to nearest with ties to even, matching
/// the selected DOSBox AUTO profile (precision control 64, rounding nearest).
/// This does not implement rounding quirks of Borland's software emulator.
///
/// Every Real48 encoding is finite: exponent-zero operands load as positive
/// zero regardless of their remaining bytes. Multiplication then retains the
/// XOR of the loaded signs, so zero times a negative nonzero operand produces
/// Extended negative zero. Nonzero products cannot reach Extended underflow
/// or overflow; conversion of the result to Real48 is a separate operation.
pub fn multiply_real48(left: Real48, right: Real48) -> Extended80 {
    let (left_significand, left_exponent, left_negative) = decode(left);
    let (right_significand, right_exponent, right_negative) = decode(right);
    let negative = left_negative ^ right_negative;

    if left_significand == 0 || right_significand == 0 {
        return encode(0, 0, negative);
    }

    let product = u128::from(left_significand) * u128::from(right_significand);
    // Each decoded significand has its integer bit at bit 39. The exact
    // product therefore has either 79 or 80 significant bits.
    let upper_normalization = u32::from(product >= (1_u128 << 79));
    let shift = 15 + upper_normalization;
    let retained = product >> shift;
    let remainder = product & ((1_u128 << shift) - 1);
    let halfway = 1_u128 << (shift - 1);
    let round_up = remainder > halfway || (remainder == halfway && retained & 1 != 0);
    let mut significand = retained + u128::from(round_up);

    // Real48 exponent bias is 129, Extended bias is 16383.
    let mut exponent = i32::from(left_exponent)
        + i32::from(right_exponent)
        + (16383 - 2 * 129)
        + upper_normalization as i32;
    if significand == (1_u128 << 64) {
        significand >>= 1;
        exponent += 1;
    }

    debug_assert!((0x3eff..=0x40fc).contains(&exponent));
    debug_assert!((1_u128 << 63..1_u128 << 64).contains(&significand));
    encode(significand as u64, exponent as u16, negative)
}

fn decode(value: Real48) -> (u64, u8, bool) {
    let bytes = value.to_le_bytes();
    if bytes[0] == 0 {
        return (0, 0, false);
    }

    let fraction = u64::from_le_bytes([
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5] & 0x7f,
        0,
        0,
        0,
    ]);
    ((1_u64 << 39) | fraction, bytes[0], bytes[5] & 0x80 != 0)
}

fn encode(significand: u64, exponent: u16, negative: bool) -> Extended80 {
    let mut bytes = [0; 10];
    bytes[..8].copy_from_slice(&significand.to_le_bytes());
    let sign_exponent = exponent | (u16::from(negative) << 15);
    bytes[8..].copy_from_slice(&sign_exponent.to_le_bytes());
    Extended80::from_le_bytes(bytes)
}

const MIN_NORMAL_EXPONENT: i32 = -16382;
const MIN_SUBNORMAL_POWER: i32 = -16445;
const MAX_EXPONENT: i32 = 16383;

#[derive(Clone, Copy)]
pub(crate) struct Finite {
    pub(crate) significand: u64,
    pub(crate) exponent: i32,
    pub(crate) negative: bool,
}

pub(crate) fn finite(value: Extended80) -> Result<Finite, NumericError> {
    let bytes = value.to_le_bytes();
    let significand = u64::from_le_bytes(bytes[..8].try_into().unwrap());
    let sign_exponent = u16::from_le_bytes([bytes[8], bytes[9]]);
    let field = sign_exponent & 0x7fff;
    let negative = sign_exponent & 0x8000 != 0;
    if field == 0 && significand == 0 {
        return Ok(Finite {
            significand: 0,
            exponent: 0,
            negative,
        });
    }
    if field == 0x7fff || (field != 0 && significand >> 63 == 0) {
        return Err(NumericError::InvalidOperation);
    }
    let normalization = significand.leading_zeros();
    Ok(Finite {
        significand: significand << normalization,
        exponent: i32::from(field.max(1)) - 16383 - normalization as i32,
        negative,
    })
}

pub(crate) fn round_shift(value: u128, shift: i32) -> u128 {
    if shift <= 0 {
        return value << (-shift as u32);
    }
    if shift > 128 {
        return 0;
    }
    let half = 1_u128 << (shift - 1);
    let retained = if shift == 128 { 0 } else { value >> shift };
    let remainder = if shift == 128 {
        value
    } else {
        value & ((1_u128 << shift) - 1)
    };
    retained + u128::from(remainder > half || (remainder == half && retained & 1 != 0))
}

/// Round an exact nonnegative integer times a power of two. AUTO masks
/// underflow, so gradual underflow and signed zero are retained, not error206.
pub(crate) fn pack(value: u128, power: i32, negative: bool) -> Result<Extended80, NumericError> {
    if value == 0 {
        return Ok(encode(0, 0, negative));
    }
    let length = 128 - value.leading_zeros() as i32;
    let mut exponent = power + length - 1;
    if exponent > MAX_EXPONENT {
        return Err(NumericError::Overflow);
    }
    if exponent < MIN_NORMAL_EXPONENT {
        let significand = round_shift(value, MIN_SUBNORMAL_POWER - power);
        return Ok(encode(
            significand as u64,
            u16::from(significand >= 1_u128 << 63),
            negative,
        ));
    }
    let mut significand = round_shift(value, length - 64);
    if significand == 1_u128 << 64 {
        significand >>= 1;
        exponent += 1;
    }
    if exponent > MAX_EXPONENT {
        return Err(NumericError::Overflow);
    }
    Ok(encode(
        significand as u64,
        (exponent + 16383) as u16,
        negative,
    ))
}

impl Extended80 {
    pub fn from_i64(value: i64) -> Self {
        pack(u128::from(value.unsigned_abs()), 0, value < 0).expect("i64 fits Extended")
    }

    // Keep the agreed method API as well as std::ops::Neg for ergonomic callers.
    #[allow(clippy::should_implement_trait)]
    pub fn neg(self) -> Self {
        let mut bytes = self.to_le_bytes();
        bytes[9] ^= 0x80;
        Self::from_le_bytes(bytes)
    }

    pub fn abs(self) -> Self {
        let mut bytes = self.to_le_bytes();
        bytes[9] &= 0x7f;
        Self::from_le_bytes(bytes)
    }

    pub fn is_zero(self) -> bool {
        let bytes = self.to_le_bytes();
        bytes[..8].iter().all(|&byte| byte == 0) && bytes[8] == 0 && bytes[9] & 0x7f == 0
    }

    pub fn compare(self, rhs: Self) -> Result<Ordering, NumericError> {
        let left = finite(self)?;
        let right = finite(rhs)?;
        if left.significand == 0 && right.significand == 0 {
            return Ok(Ordering::Equal);
        }
        if left.negative != right.negative {
            return Ok(if left.negative {
                Ordering::Less
            } else {
                Ordering::Greater
            });
        }
        let order = if left.significand == 0 {
            Ordering::Less
        } else if right.significand == 0 {
            Ordering::Greater
        } else {
            (left.exponent, left.significand).cmp(&(right.exponent, right.significand))
        };
        Ok(if left.negative {
            order.reverse()
        } else {
            order
        })
    }

    pub fn checked_add(self, rhs: Self) -> Result<Self, NumericError> {
        let mut left = finite(self)?;
        let mut right = finite(rhs)?;
        if left.significand == 0 && right.significand == 0 {
            return Ok(encode(0, 0, left.negative && right.negative));
        }
        if left.significand == 0 {
            return pack(
                u128::from(right.significand),
                right.exponent - 63,
                right.negative,
            );
        }
        if right.significand == 0 {
            return pack(
                u128::from(left.significand),
                left.exponent - 63,
                left.negative,
            );
        }
        if (left.exponent, left.significand) < (right.exponent, right.significand) {
            std::mem::swap(&mut left, &mut right);
        }
        let difference = left.exponent - right.exponent;
        if difference > 64 {
            // Below a power of two, the adjacent normal spacing is halved.
            if difference == 65
                && left.negative != right.negative
                && left.significand == 1_u64 << 63
                && right.significand > 1_u64 << 63
                && left.exponent > MIN_NORMAL_EXPONENT
            {
                return pack(u128::from(u64::MAX), left.exponent - 64, left.negative);
            }
            return pack(
                u128::from(left.significand),
                left.exponent - 63,
                left.negative,
            );
        }
        let aligned = u128::from(left.significand) << difference;
        let magnitude = if left.negative == right.negative {
            aligned + u128::from(right.significand)
        } else {
            aligned - u128::from(right.significand)
        };
        pack(
            magnitude,
            right.exponent - 63,
            magnitude != 0 && left.negative,
        )
    }

    pub fn checked_sub(self, rhs: Self) -> Result<Self, NumericError> {
        self.checked_add(rhs.neg())
    }

    pub fn checked_mul(self, rhs: Self) -> Result<Self, NumericError> {
        let left = finite(self)?;
        let right = finite(rhs)?;
        pack(
            u128::from(left.significand) * u128::from(right.significand),
            left.exponent + right.exponent - 126,
            left.negative ^ right.negative,
        )
    }

    pub fn checked_div(self, rhs: Self) -> Result<Self, NumericError> {
        let left = finite(self)?;
        let right = finite(rhs)?;
        let negative = left.negative ^ right.negative;
        if right.significand == 0 {
            return Err(if left.significand == 0 {
                NumericError::InvalidOperation
            } else {
                NumericError::DivisionByZero
            });
        }
        if left.significand == 0 {
            return Ok(encode(0, 0, negative));
        }
        let lower = left.significand < right.significand;
        let mut exponent = left.exponent - right.exponent - i32::from(lower);
        if exponent > MAX_EXPONENT {
            return Err(NumericError::Overflow);
        }
        let shift = 63 + i32::from(lower) - (MIN_NORMAL_EXPONENT - exponent).max(0);
        if shift < -64 {
            return Ok(encode(0, 0, negative));
        }
        let (numerator, denominator) = if shift >= 0 {
            (
                u128::from(left.significand) << shift,
                u128::from(right.significand),
            )
        } else {
            (
                u128::from(left.significand),
                u128::from(right.significand) << -shift,
            )
        };
        let retained = numerator / denominator;
        let remainder = numerator % denominator;
        // Avoid doubling a potentially 128-bit denominator/remainder.
        let reverse = denominator - remainder;
        let mut rounded = retained
            + u128::from(remainder > reverse || (remainder == reverse && retained & 1 != 0));
        if exponent < MIN_NORMAL_EXPONENT {
            return Ok(encode(
                rounded as u64,
                u16::from(rounded >= 1_u128 << 63),
                negative,
            ));
        }
        if rounded == 1_u128 << 64 {
            rounded >>= 1;
            exponent += 1;
        }
        if exponent > MAX_EXPONENT {
            return Err(NumericError::Overflow);
        }
        Ok(encode(rounded as u64, (exponent + 16383) as u16, negative))
    }

    pub fn checked_sqrt(self) -> Result<Self, NumericError> {
        let input = finite(self)?;
        if input.significand == 0 {
            return Ok(encode(0, 0, input.negative));
        }
        if input.negative {
            return Err(NumericError::InvalidOperation);
        }
        let odd = input.exponent.rem_euclid(2);
        let radicand = u128::from(input.significand) << (63 + odd);
        let mut root = 0_u128;
        let mut remainder = 0_u128;
        for pair in (0..64).rev() {
            remainder = (remainder << 2) | ((radicand >> (pair * 2)) & 3);
            let trial = (root << 2) | 1;
            root <<= 1;
            if remainder >= trial {
                remainder -= trial;
                root |= 1;
            }
        }
        let rounded = root + u128::from(remainder > root);
        pack(rounded, input.exponent.div_euclid(2) - 63, false)
    }

    /// Lossy conversion solely for drawing and display. Never use its result
    /// as an operand of the compatibility calculation.
    pub fn to_display_f64(self) -> f64 {
        let Ok(input) = finite(self) else {
            return f64::NAN;
        };
        let sign = u64::from(input.negative) << 63;
        if input.significand == 0 {
            return f64::from_bits(sign);
        }
        if input.exponent > 1023 {
            return f64::from_bits(sign | 0x7ff0_0000_0000_0000);
        }
        if input.exponent < -1022 {
            let significand =
                round_shift(u128::from(input.significand), -1074 - (input.exponent - 63));
            return f64::from_bits(sign | significand as u64);
        }
        let mut significand = round_shift(u128::from(input.significand), 11);
        let mut exponent = input.exponent;
        if significand == 1_u128 << 53 {
            significand >>= 1;
            exponent += 1;
        }
        if exponent > 1023 {
            return f64::from_bits(sign | 0x7ff0_0000_0000_0000);
        }
        f64::from_bits(
            sign | ((exponent + 1023) as u64) << 52 | (significand as u64 & ((1_u64 << 52) - 1)),
        )
    }
}

impl std::ops::Neg for Extended80 {
    type Output = Self;
    fn neg(self) -> Self {
        Extended80::neg(self)
    }
}
