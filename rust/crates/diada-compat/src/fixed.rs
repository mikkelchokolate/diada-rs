//! Internal unsigned Q192 arithmetic for transcendental evaluation.
use crate::arithmetic::{finite, pack};
use crate::{Extended80, NumericError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Fixed([u64; 4]);

impl Ord for Fixed {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.iter().rev().cmp(other.0.iter().rev())
    }
}
impl PartialOrd for Fixed {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Fixed {
    pub(crate) const ZERO: Self = Self([0; 4]);
    pub(crate) const ONE: Self = Self([0, 0, 0, 1]);
    pub(crate) fn binary(value: u128, power: i32) -> Self {
        let shift = power + 192;
        let mut result = Self::ZERO;
        for bit in 0..128 {
            if value & (1_u128 << bit) != 0 {
                let at = bit + shift;
                if (0..256).contains(&at) {
                    result.0[at as usize / 64] |= 1_u64 << (at % 64);
                }
            }
        }
        result
    }
    pub(crate) fn high_bit(self) -> Option<i32> {
        (0..4)
            .rev()
            .find(|&i| self.0[i] != 0)
            .map(|i| i as i32 * 64 + 63 - self.0[i].leading_zeros() as i32)
    }
    pub(crate) fn rounded(self, precision: i32, nearest: bool) -> Self {
        let Some(high) = self.high_bit() else {
            return self;
        };
        let shift = high + 1 - precision;
        if shift <= 0 {
            return self;
        }
        let half_at = (shift - 1) as usize;
        let half = self.0[half_at / 64] & (1_u64 << (half_at % 64)) != 0;
        let sticky = (0..half_at).any(|i| self.0[i / 64] & (1_u64 << (i % 64)) != 0);
        let odd = self.0[shift as usize / 64] & (1_u64 << (shift % 64)) != 0;
        let mut out = self;
        for i in 0..shift as usize {
            out.0[i / 64] &= !(1_u64 << (i % 64));
        }
        if nearest && half && (sticky || odd) {
            let mut delta = Self::ZERO;
            delta.0[shift as usize / 64] = 1_u64 << (shift % 64);
            out = out.add(delta);
        }
        out
    }
    pub(crate) fn from_extended(value: Extended80) -> Result<Self, NumericError> {
        let f = finite(value)?;
        if f.significand == 0 {
            return Ok(Self::ZERO);
        }
        let shift = f.exponent + 129;
        let mut limbs = [0; 4];
        if shift >= 0 {
            let word = shift as usize / 64;
            let bit = shift as usize % 64;
            if word >= 4 {
                return Err(NumericError::Overflow);
            }
            limbs[word] = f.significand << bit;
            if bit != 0 && word + 1 < 4 {
                limbs[word + 1] = f.significand >> (64 - bit);
            }
        } else if shift > -64 {
            limbs[0] = f.significand >> (-shift);
        }
        Ok(Self(limbs))
    }
    pub(crate) fn to_extended(self) -> Result<Extended80, NumericError> {
        let top = (0..4).rev().find(|&i| self.0[i] != 0);
        let Some(top) = top else {
            return Ok(Extended80::ZERO);
        };
        if top == 0 {
            return pack(u128::from(self.0[0]), -192, false);
        }
        let pair = u128::from(self.0[top - 1]) | (u128::from(self.0[top]) << 64);
        let length = 128 - pair.leading_zeros() as i32;
        let shift = length - 64;
        let retained = pair >> shift;
        let half = 1_u128 << (shift - 1);
        let rest = pair & ((1_u128 << shift) - 1);
        let sticky = self.0[..top - 1].iter().any(|&x| x != 0);
        let rounded =
            retained + u128::from(rest > half || (rest == half && (sticky || retained & 1 != 0)));
        pack(rounded, (top as i32 - 1) * 64 - 192 + shift, false)
    }
    pub(crate) fn add(self, rhs: Self) -> Self {
        let mut out = [0; 4];
        let mut carry = 0_u128;
        for (i, v) in out.iter_mut().enumerate() {
            let sum = u128::from(self.0[i]) + u128::from(rhs.0[i]) + carry;
            *v = sum as u64;
            carry = sum >> 64;
        }
        assert_eq!(carry, 0, "Q192 addition overflow");
        Self(out)
    }
    pub(crate) fn sub(self, rhs: Self) -> Self {
        assert!(self >= rhs, "Q192 subtraction sign");
        let mut out = [0; 4];
        let mut borrow = false;
        for (i, v) in out.iter_mut().enumerate() {
            let (a, b) = self.0[i].overflowing_sub(rhs.0[i]);
            let (c, d) = a.overflowing_sub(u64::from(borrow));
            *v = c;
            borrow = b || d;
        }
        Self(out)
    }
    pub(crate) fn mul(self, rhs: Self) -> Self {
        let mut out = [0_u64; 8];
        for i in 0..4 {
            let mut carry = 0_u128;
            for j in 0..4 {
                let k = i + j;
                let v = u128::from(self.0[i]) * u128::from(rhs.0[j]) + u128::from(out[k]) + carry;
                out[k] = v as u64;
                carry = v >> 64;
            }
            out[i + 4] = carry as u64;
        }
        assert_eq!(out[7], 0, "Q192 multiplication overflow");
        Self(out[3..7].try_into().unwrap())
    }
    pub(crate) fn div(self, rhs: Self) -> Self {
        assert_ne!(rhs, Self::ZERO);
        let numerator = [0, 0, 0, self.0[0], self.0[1], self.0[2], self.0[3], 0];
        let mut remainder = Self::ZERO;
        let mut quotient = [0; 4];
        for bit in (0..448).rev() {
            let mut carry = (numerator[bit / 64] >> (bit % 64)) & 1;
            for limb in &mut remainder.0 {
                let next = *limb >> 63;
                *limb = (*limb << 1) | carry;
                carry = next;
            }
            assert_eq!(carry, 0);
            if remainder >= rhs {
                remainder = remainder.sub(rhs);
                assert!(bit < 256, "Q192 division overflow");
                quotient[bit / 64] |= 1_u64 << (bit % 64);
            }
        }
        Self(quotient)
    }
}
