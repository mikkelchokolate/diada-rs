use std::fmt;

/// The six-byte Turbo Pascal Real encoding, in original memory order.
///
/// Byte zero holds the biased exponent. The remaining bytes hold 39 fraction
/// bits and a sign bit. An exponent of zero represents zero. Equality compares
/// encodings; noncanonical encodings of zero remain distinct until loaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Real48([u8; 6]);

impl Real48 {
    pub const ZERO: Self = Self([0; 6]);

    pub const fn from_le_bytes(bytes: [u8; 6]) -> Self {
        Self(bytes)
    }

    pub const fn to_le_bytes(self) -> [u8; 6] {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0[0] == 0
    }

    /// Reproduce original SYSTEM:31D5..31FA without native floating point.
    pub fn load_extended(self) -> Extended80 {
        if self.is_zero() {
            return Extended80::ZERO;
        }

        let fraction = u64::from_le_bytes([
            self.0[1],
            self.0[2],
            self.0[3],
            self.0[4],
            self.0[5] & 0x7f,
            0,
            0,
            0,
        ]);
        let significand = ((1_u64 << 39) | fraction) << 24;
        let exponent = u16::from(self.0[0]) + 0x3f7e;
        let sign = u16::from(self.0[5] & 0x80) << 8;
        Extended80::from_parts(significand, exponent | sign)
    }
}

/// Raw ten-byte Extended encoding; an explicit 64-bit significand and exponent.
///
/// This type preserves original bytes. Checked arithmetic follows the selected
/// DOSBox AUTO precision-64, nearest-even profile and rejects nonfinite values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Extended80([u8; 10]);

impl Extended80 {
    pub const ZERO: Self = Self([0; 10]);

    pub const fn from_le_bytes(bytes: [u8; 10]) -> Self {
        Self(bytes)
    }

    pub const fn to_le_bytes(self) -> [u8; 10] {
        self.0
    }

    fn from_parts(significand: u64, sign_exponent: u16) -> Self {
        let mut bytes = [0; 10];
        bytes[..8].copy_from_slice(&significand.to_le_bytes());
        bytes[8..].copy_from_slice(&sign_exponent.to_le_bytes());
        Self(bytes)
    }

    /// Reproduce the conversion in original SYSTEM:31FF..3245.
    ///
    /// The supported contract is finite normalized Extended values and zero.
    /// Raw unsupported/nonfinite encodings require the original FPU validation,
    /// which is not implemented by this storage conversion.
    ///
    /// The exponent range is checked before rounding. Values below the Real48
    /// range become canonical zero. The leading discarded significand bit
    /// controls rounding, including exact halves away from zero. Overflow
    /// corresponds to Turbo Pascal runtime error 205.
    pub fn store_real(self) -> Result<Real48, NumericError> {
        let sign_exponent = u16::from_le_bytes([self.0[8], self.0[9]]);
        let mut exponent = i32::from(sign_exponent & 0x7fff) - 0x3f7e;
        if exponent <= 0 {
            return Ok(Real48::ZERO);
        }
        if exponent > 255 {
            return Err(NumericError::Overflow);
        }

        let significand = u64::from_le_bytes(self.0[..8].try_into().unwrap());
        let round_up = u64::from(significand & (1 << 23) != 0);
        let rounded = (significand >> 24) + round_up;
        if rounded == 1 << 40 {
            exponent += 1;
            if exponent > 255 {
                return Err(NumericError::Overflow);
            }
        }

        let fraction = (rounded & ((1 << 39) - 1)).to_le_bytes();
        let sign = self.0[9] & 0x80;
        Ok(Real48([
            exponent as u8,
            fraction[0],
            fraction[1],
            fraction[2],
            fraction[3],
            fraction[4] | sign,
        ]))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericError {
    Overflow,
    Underflow,
    InvalidOperation,
    DivisionByZero,
}

impl NumericError {
    pub const fn runtime_code(self) -> u16 {
        match self {
            Self::Overflow => 205,
            Self::Underflow => 206,
            Self::InvalidOperation => 207,
            Self::DivisionByZero => 200,
        }
    }
}

impl fmt::Display for NumericError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Overflow => f.write_str("floating-point overflow (Turbo Pascal error 205)"),
            Self::Underflow => f.write_str("floating-point underflow (Turbo Pascal error 206)"),
            Self::InvalidOperation => {
                f.write_str("invalid floating-point operation (Turbo Pascal error 207)")
            }
            Self::DivisionByZero => {
                f.write_str("floating-point division by zero (Turbo Pascal error 200)")
            }
        }
    }
}

impl std::error::Error for NumericError {}
