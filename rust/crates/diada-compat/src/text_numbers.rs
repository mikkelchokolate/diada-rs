use crate::arithmetic::{finite, pack, round_shift};
use crate::{Extended80, NumericError, Real48};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValError {
    /// One-based TP Val position; zero denotes an arithmetic failure after
    /// syntactically successful parsing and conversion to six-byte Real.
    pub position: usize,
    pub numeric_error: Option<NumericError>,
}

impl fmt::Display for ValError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(error) = self.numeric_error {
            write!(f, "{error}")
        } else {
            write!(f, "invalid number at position {}", self.position)
        }
    }
}
impl std::error::Error for ValError {}

const POWERS: [[u8; 10]; 10] = [
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0xbc, 0xbe, 0x19, 0x40],
    [0x00, 0x00, 0x00, 0x04, 0xbf, 0xc9, 0x1b, 0x8e, 0x34, 0x40],
    [0x9e, 0xb5, 0x70, 0x2b, 0xa8, 0xad, 0xc5, 0x9d, 0x69, 0x40],
    [0xd5, 0xa6, 0xcf, 0xff, 0x49, 0x1f, 0x78, 0xc2, 0xd3, 0x40],
    [0xe0, 0x8c, 0xe9, 0x80, 0xc9, 0x47, 0xba, 0x93, 0xa8, 0x41],
    [0x8e, 0xde, 0xf9, 0x9d, 0xfb, 0xeb, 0x7e, 0xaa, 0x51, 0x43],
    [0xc7, 0x91, 0x0e, 0xa6, 0xae, 0xa0, 0x19, 0xe3, 0xa3, 0x46],
    [0x17, 0x0c, 0x75, 0x81, 0x86, 0x75, 0x76, 0xc9, 0x48, 0x4d],
    [0xe5, 0x5d, 0x3d, 0xc5, 0x5d, 0x3b, 0x8b, 0x9e, 0x92, 0x5a],
    [0x9b, 0x97, 0x20, 0x8a, 0x02, 0x52, 0x60, 0xc4, 0x25, 0x75],
];

fn scale(mut value: Extended80, exponent: i32) -> Result<Extended80, NumericError> {
    let negative = exponent < 0;
    let mut power = exponent.unsigned_abs();
    // Original SYSTEM:35D1 handles one extra 4096 before its bit-table walk.
    if power > 4096 {
        let factor = Extended80::from_le_bytes(POWERS[9]);
        value = if negative {
            value.checked_div(factor)?
        } else {
            value.checked_mul(factor)?
        };
        power -= 4096;
    }
    let mut factor = Extended80::from_i64(10_i64.pow(power & 7));
    power >>= 3;
    let mut index = 0;
    while power != 0 {
        if power & 1 != 0 {
            factor = factor.checked_mul(Extended80::from_le_bytes(
                *POWERS.get(index).ok_or(NumericError::Overflow)?,
            ))?;
        }
        index += 1;
        power >>= 1;
    }
    if negative {
        value.checked_div(factor)
    } else {
        value.checked_mul(factor)
    }
}

pub fn parse_real(input: &[u8]) -> Result<Real48, ValError> {
    let syntax = |position| ValError {
        position: position + 1,
        numeric_error: None,
    };
    let mut cursor = 0;
    while input.get(cursor) == Some(&b' ') {
        cursor += 1;
    }
    let negative = input.get(cursor) == Some(&b'-');
    if matches!(input.get(cursor), Some(b'+' | b'-')) {
        cursor += 1;
    }
    let start = cursor;
    let mut value = Extended80::ZERO;
    let ten = Extended80::from_i64(10);
    while let Some(digit @ b'0'..=b'9') = input.get(cursor) {
        value = value
            .checked_mul(ten)
            .and_then(|v| v.checked_add(Extended80::from_i64(i64::from(digit - b'0'))))
            .map_err(|_| syntax(cursor))?;
        cursor += 1;
    }
    let mut exponent = 0_i32;
    if input.get(cursor) == Some(&b'.') {
        cursor += 1;
        while let Some(digit @ b'0'..=b'9') = input.get(cursor) {
            value = value
                .checked_mul(ten)
                .and_then(|v| v.checked_add(Extended80::from_i64(i64::from(digit - b'0'))))
                .map_err(|_| syntax(cursor))?;
            cursor += 1;
            exponent -= 1;
        }
    }
    // TP accepts '.' as a zero value; the point consumes input like a digit.
    if cursor == start {
        return Err(syntax(cursor));
    }
    if matches!(input.get(cursor), Some(b'e' | b'E')) {
        cursor += 1;
        let exp_negative = input.get(cursor) == Some(&b'-');
        if matches!(input.get(cursor), Some(b'+' | b'-')) {
            cursor += 1;
        }
        let hexadecimal = input.get(cursor) == Some(&b'$');
        if hexadecimal {
            cursor += 1;
        }
        let exp_start = cursor;
        let radix = if hexadecimal { 16 } else { 10 };
        let mut explicit = 0_u32;
        while let Some(&byte) = input.get(cursor) {
            let digit = match byte {
                b'0'..=b'9' => u32::from(byte - b'0'),
                b'a'..=b'f' if hexadecimal => u32::from(byte - b'a') + 10,
                b'A'..=b'F' if hexadecimal => u32::from(byte - b'A') + 10,
                _ => break,
            };
            explicit = explicit
                .checked_mul(radix)
                .and_then(|e| e.checked_add(digit))
                .filter(|&e| hexadecimal || e <= 0x8000_0000)
                .ok_or_else(|| syntax(cursor))?;
            cursor += 1;
        }
        if cursor == exp_start {
            return Err(syntax(cursor));
        }
        let explicit = if exp_negative {
            (explicit as i32).wrapping_neg()
        } else {
            explicit as i32
        };
        if explicit.unsigned_abs() >= 4999 {
            return Err(syntax(cursor));
        }
        exponent += explicit;
    }
    if cursor != input.len() {
        return Err(syntax(cursor));
    }
    value = scale(value, exponent).map_err(|_| syntax(cursor))?;
    if negative {
        value = value.neg();
    }
    value.store_real().map_err(|error| ValError {
        position: 0,
        numeric_error: Some(error),
    })
}

pub fn format_real(
    value: Real48,
    width: i16,
    decimals: Option<i16>,
) -> Result<String, NumericError> {
    format_extended(value.load_extended(), width, decimals)
}

pub fn format_extended(
    value: Extended80,
    width: i16,
    decimals: Option<i16>,
) -> Result<String, NumericError> {
    let input = finite(value)?;
    let mut exponent = 0_i32;
    let mut digits = [0_u8; 18];
    let raw = value.to_le_bytes();
    // Original Str checks the encoded exponent before inspecting the fraction;
    // every exponent-zero encoding, including a subnormal, formats as zero.
    if u16::from_le_bytes([raw[8], raw[9]]) & 0x7fff != 0 {
        exponent = (input.exponent * 0x4d10) >> 16;
        let scaled = scale(value.abs(), 17 - exponent)?;
        let scaled = finite(scaled)?;
        let mut integer = round_shift(u128::from(scaled.significand), 63 - scaled.exponent);
        if integer >= 1_000_000_000_000_000_000 {
            let divided = finite(pack(integer, 0, false)?.checked_div(Extended80::from_i64(10))?)?;
            integer = round_shift(u128::from(divided.significand), 63 - divided.exponent);
            exponent += 1;
        }
        let text = format!("{integer:018}");
        for (digit, byte) in digits.iter_mut().zip(text.bytes()) {
            *digit = byte - b'0';
        }
    }
    let fixed = decimals
        .filter(|&d| d >= 0)
        .map(|d| i32::from(d.min(18)))
        .filter(|_| exponent < 36);
    let precision = fixed.unwrap_or_else(|| {
        if exponent >= 36 && decimals.is_some_and(|d| d >= 0) {
            18
        } else {
            i32::from(width.wrapping_sub(8)).clamp(2, 18)
        }
    });
    let kept = if fixed.is_some() {
        precision + exponent + 1
    } else {
        precision
    };
    if kept < 0 {
        digits.fill(0);
    } else if kept < 18 {
        let kept = kept as usize;
        let round_up = digits[kept] >= 5;
        digits[kept..].fill(0);
        if round_up {
            let mut at = kept;
            loop {
                if at == 0 {
                    digits.fill(0);
                    digits[0] = 1;
                    exponent += 1;
                    break;
                }
                at -= 1;
                digits[at] += 1;
                if digits[at] <= 9 {
                    break;
                }
                digits[at] = 0;
            }
        }
    }
    let digit_at = |index: i32| -> char {
        let digit = usize::try_from(index)
            .ok()
            .and_then(|i| digits.get(i))
            .copied()
            .unwrap_or(0);
        char::from(b'0' + digit)
    };
    let mut text = String::new();
    if fixed.is_some() {
        if input.negative {
            text.push('-');
        }
        if exponent < 0 {
            text.push('0');
        } else {
            for index in 0..=exponent {
                text.push(digit_at(index));
            }
        }
        if precision != 0 {
            text.push('.');
            for index in 1..=precision {
                text.push(digit_at(exponent + index));
            }
        }
    } else {
        text.push(if input.negative { '-' } else { ' ' });
        text.push(digit_at(0));
        text.push('.');
        for index in 1..precision {
            text.push(digit_at(index));
        }
        text.push('E');
        text.push(if exponent < 0 { '-' } else { '+' });
        text.push_str(&format!("{:04}", exponent.unsigned_abs()));
    }
    let padding = (usize::from(width.max(0) as u16).min(255)).saturating_sub(text.len());
    Ok(" ".repeat(padding) + &text)
}
