//! Rust derivative of the Bochs Intel P5/P6 x87 transcendental model.
//! The original permission notice is preserved in BOCHS-NUMERIC-NOTICE.txt.
//! Internal arithmetic widths are explicit and use integer Q192 operations.
use crate::arithmetic::finite;
use crate::fixed::Fixed;
use crate::native_trig_constants::*;
use crate::{Extended80, NumericError};

#[derive(Clone, Copy)]
struct Wide {
    magnitude: Fixed,
    negative: bool,
}
impl Wide {
    fn one() -> Self {
        Self {
            magnitude: Fixed::ONE,
            negative: false,
        }
    }
    fn from(value: Extended80) -> Result<Self, NumericError> {
        Ok(Self {
            magnitude: Fixed::from_extended(value.abs())?,
            negative: finite(value)?.negative,
        })
    }
    fn encoded((hi, lo): Encoded) -> Self {
        if hi & 0x7fff_ffff_ffff_ffff == 0 && lo == 0 {
            return Self {
                magnitude: Fixed::ZERO,
                negative: false,
            };
        }
        let exponent = ((hi >> 48) & 0x7fff) as i32 - 16383;
        let significand =
            (1_u128 << 112) | (u128::from(hi & 0x0000_ffff_ffff_ffff) << 64) | u128::from(lo);
        Self {
            magnitude: Fixed::binary(significand, exponent - 112),
            negative: hi >> 63 != 0,
        }
    }
    fn neg(mut self) -> Self {
        self.negative = !self.negative;
        self
    }
    fn round(mut self, precision: i32, nearest: bool) -> Self {
        self.magnitude = self.magnitude.rounded(precision, nearest);
        self
    }
    fn add(self, rhs: Self) -> Self {
        if self.negative == rhs.negative {
            Self {
                magnitude: self.magnitude.add(rhs.magnitude),
                negative: self.negative,
            }
        } else if self.magnitude >= rhs.magnitude {
            Self {
                magnitude: self.magnitude.sub(rhs.magnitude),
                negative: self.negative,
            }
        } else {
            Self {
                magnitude: rhs.magnitude.sub(self.magnitude),
                negative: rhs.negative,
            }
        }
    }
    fn mul(self, rhs: Self) -> Self {
        Self {
            magnitude: self.magnitude.mul(rhs.magnitude),
            negative: self.negative ^ rhs.negative,
        }
    }
    fn div(self, rhs: Self) -> Self {
        Self {
            magnitude: self.magnitude.div(rhs.magnitude),
            negative: self.negative ^ rhs.negative,
        }
    }
    fn add67(self, rhs: Self) -> Self {
        self.add(rhs).round(67, false)
    }
    fn add64(self, rhs: Self) -> Self {
        self.add(rhs).round(113, true).round(64, true)
    }
    fn mul67(self, rhs: Self) -> Self {
        self.mul(rhs).round(67, false)
    }
    fn mule67(self, rhs: Self) -> Self {
        self.mul67(rhs.round(64, false))
    }
    fn mul64(self, rhs: Self) -> Self {
        self.mul(rhs.round(64, false))
            .round(113, true)
            .round(64, true)
    }
    fn div67(self, rhs: Self) -> Self {
        self.div(rhs).round(67, false)
    }
    fn finish(self) -> Result<Extended80, NumericError> {
        self.round(113, true)
            .magnitude
            .to_extended()
            .map(|x| if self.negative { x.neg() } else { x })
    }
}

pub(crate) fn tangent(value: Extended80) -> Result<Extended80, NumericError> {
    let decoded = finite(value)?;
    if decoded.significand == 0 || decoded.exponent < -68 {
        return Ok(value);
    }
    let x = Wide::from(value.abs())?;
    let (mut f, g) = if decoded.exponent >= -2 {
        let b1 = (decoded.significand >> 62) & 1;
        let b2 = (decoded.significand >> 61) & 1;
        let index =
            ((((decoded.exponent + 16383) & 1) as usize) << 2) | ((b1 as usize) << 1) | b2 as usize;
        let significand = (decoded.significand & 0xe000_0000_0000_0000) | 0x1000_0000_0000_0000;
        let c = Wide {
            magnitude: Fixed::binary(u128::from(significand), decoded.exponent - 63),
            negative: false,
        };
        let d = x.add67(c.neg());
        let e = d.mul67(d);
        let mut p = Wide::encoded(SC_U[3]);
        let mut q = Wide::encoded(SC_V[3]);
        for i in (0..3).rev() {
            p = e.mul67(p).add64(Wide::encoded(SC_U[i]));
            q = e.mul67(q).add64(Wide::encoded(SC_V[i]));
        }
        p = e.mul67(p);
        p = d.add64(p.mul67(d));
        q = e.mul64(q);
        let sigma = Wide::encoded(SC_SIN[index]);
        let gamma = Wide::encoded(SC_COS[index]);
        let f = sigma.add67(gamma.mul67(p).add67(sigma.mul67(q)));
        let g = gamma.add67(sigma.mul67(p).neg().add67(gamma.mul67(q)));
        (f, g)
    } else {
        let s = x.mule67(x);
        let mut p = Wide::encoded(SC_A[5]);
        let mut q = Wide::encoded(SC_B[5]);
        for i in (0..5).rev() {
            p = s.mul67(p).add64(Wide::encoded(SC_A[i]));
            q = s.mul67(q).add64(Wide::encoded(SC_B[i]));
        }
        p = s.mul64(p);
        q = s.mul67(q);
        (x.add67(x.mul67(p)), Wide::one().add67(q))
    };
    if decoded.negative {
        f = f.neg();
    }
    f.div(g).finish()
}

pub(crate) fn atan_ratio(
    numerator: Extended80,
    denominator: Extended80,
) -> Result<Extended80, NumericError> {
    let n = finite(numerator.abs())?;
    let d = finite(denominator.abs())?;
    if n.significand == 0 {
        return Ok(numerator);
    }
    if n.exponent - d.exponent < -100 {
        return numerator.checked_div(denominator);
    }
    let v = Wide {
        magnitude: Fixed::binary(u128::from(n.significand), n.exponent - d.exponent - 63),
        negative: false,
    };
    let u = Wide {
        magnitude: Fixed::binary(u128::from(d.significand), -63),
        negative: false,
    };
    if n.exponent - d.exponent < -40 {
        return v.div67(u).finish();
    }
    // Exact break-point reduction: nearest integer k to 32*v.
    let thirty_two = Wide {
        magnitude: Fixed::binary(32, 0),
        negative: false,
    };
    let v32 = v.mul(thirty_two);
    let k = (0..=32)
        .min_by_key(|&i| {
            let candidate = u.magnitude.mul(Fixed::binary(i as u128, 0));
            if candidate >= v32.magnitude {
                candidate.sub(v32.magnitude)
            } else {
                v32.magnitude.sub(candidate)
            }
        })
        .unwrap();
    let r = v32.add(
        u.mul(Wide {
            magnitude: Fixed::binary(k as u128, 0),
            negative: false,
        })
        .neg(),
    );
    let (f, g) = if k > 1 {
        let s = u.mul(thirty_two).add67(v.mul67(Wide {
            magnitude: Fixed::binary(k as u128, 0),
            negative: false,
        }));
        let z = r.div67(s);
        let c = z.mul64(z);
        let h = c.mul67(c);
        let t = z.mul67(c);
        let p = h
            .mul67(Wide::encoded(AT_A[2]))
            .add67(Wide::encoded(AT_A[0]));
        let q = h
            .mul67(Wide::encoded(AT_A[3]))
            .add64(Wide::encoded(AT_A[1]));
        let q = c.mul67(q);
        (Wide::encoded(AT_TABLE[k]), z.add67(t.mul67(p.add64(q))))
    } else {
        let z = v.div67(u);
        let c = z.mul64(z);
        let h = c.mul67(c);
        let t = z.mul67(c);
        let p = h
            .mul67(Wide::encoded(AT_B[4]))
            .add64(Wide::encoded(AT_B[2]));
        let p = h.mul67(p).add67(Wide::encoded(AT_B[0]));
        let q = h
            .mul67(Wide::encoded(AT_B[5]))
            .add64(Wide::encoded(AT_B[3]));
        let q = h.mul67(q).add67(Wide::encoded(AT_B[1]));
        let q = q.mul67(c);
        (z, t.mul67(p.add64(q)))
    };
    f.add(g).finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_measured_native_primitives() {
        let hex = |text: &str| -> [u8; 10] {
            std::array::from_fn(|i| u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap())
        };
        let mut count = 0;
        for row in include_str!("../fixtures/trig-primitives.tsv")
            .lines()
            .skip(1)
        {
            let fields: Vec<_> = row.split('\t').collect();
            let input = Extended80::from_le_bytes(hex(fields[2]));
            let output = if fields[1] == "T" {
                tangent(input)
            } else {
                atan_ratio(input.abs(), Extended80::from_i64(1)).map(|x| {
                    if finite(input).unwrap().negative {
                        x.neg()
                    } else {
                        x
                    }
                })
            };
            assert_eq!(
                output.unwrap().to_le_bytes(),
                hex(fields[3]),
                "{}",
                fields[0]
            );
            count += 1;
        }
        assert_eq!(count, 2758);
    }
}
