use diada_compat::{Extended80, NumericError, Real48};

// Byte expectations below follow original SYSTEM:31D5 and :31FF. The product
// case is additionally measured in TP7.1 DOSBox, hardware and software paths.
#[test]
fn measured_extended_product_stores_original_real48() {
    let product =
        Extended80::from_le_bytes([0x16, 0xfd, 0x7f, 0x98, 0xa9, 0x66, 0xe5, 0x82, 0x00, 0x40]);
    assert_eq!(
        product.store_real().unwrap().to_le_bytes(),
        [0x82, 0x98, 0xa9, 0x66, 0xe5, 0x02]
    );
    // Rounding an f64 intermediate would instead produce ...99 A9... .
}

#[test]
fn halfway_rounds_away_from_zero() {
    for (sign, expected_sign) in [(0x3f, 0), (0xbf, 0x80)] {
        let halfway = Extended80::from_le_bytes([0, 0, 0x80, 0, 0, 0, 0, 0x80, 0xff, sign]);
        assert_eq!(
            halfway.store_real().unwrap().to_le_bytes(),
            [0x81, 1, 0, 0, 0, expected_sign]
        );
    }
}

#[test]
fn neighbours_of_halfway_choose_adjacent_real48_values() {
    let below = Extended80::from_le_bytes([0xff, 0xff, 0x7f, 0, 0, 0, 0, 0x80, 0xff, 0x3f]);
    let above = Extended80::from_le_bytes([1, 0, 0x80, 0, 0, 0, 0, 0x80, 0xff, 0x3f]);
    assert_eq!(
        below.store_real().unwrap().to_le_bytes(),
        [0x81, 0, 0, 0, 0, 0]
    );
    assert_eq!(
        above.store_real().unwrap().to_le_bytes(),
        [0x81, 1, 0, 0, 0, 0]
    );
}

#[test]
fn rounding_carries_into_exponent() {
    let almost_two =
        Extended80::from_le_bytes([0, 0, 0x80, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x3f]);
    assert_eq!(
        almost_two.store_real().unwrap().to_le_bytes(),
        [0x82, 0, 0, 0, 0, 0]
    );
}

#[test]
fn underflow_happens_before_rounding_at_lower_boundary() {
    let below_minimum =
        Extended80::from_le_bytes([0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7e, 0x3f]);
    let minimum = Extended80::from_le_bytes([0, 0, 0, 0, 0, 0, 0, 0x80, 0x7f, 0x3f]);
    assert_eq!(below_minimum.store_real(), Ok(Real48::ZERO));
    assert_eq!(
        minimum.store_real().unwrap().to_le_bytes(),
        [1, 0, 0, 0, 0, 0]
    );
}

#[test]
fn maximum_is_valid_but_rounding_beyond_it_is_error_205() {
    let maximum = Extended80::from_le_bytes([0, 0, 0, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7d, 0x40]);
    let overflow =
        Extended80::from_le_bytes([0, 0, 0x80, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7d, 0x40]);
    assert_eq!(
        maximum.store_real().unwrap().to_le_bytes(),
        [0xff, 0xff, 0xff, 0xff, 0xff, 0x7f]
    );
    assert_eq!(overflow.store_real(), Err(NumericError::Overflow));
    assert_eq!(NumericError::Overflow.runtime_code(), 205);
}

#[test]
fn load_preserves_sign_and_fraction_without_rounding() {
    let one = Real48::from_le_bytes([0x81, 0, 0, 0, 0, 0]);
    let negative = Real48::from_le_bytes([0x81, 1, 0, 0, 0, 0x80]);
    assert_eq!(
        one.load_extended().to_le_bytes(),
        [0, 0, 0, 0, 0, 0, 0, 0x80, 0xff, 0x3f]
    );
    assert_eq!(
        negative.load_extended().to_le_bytes(),
        [0, 0, 0, 1, 0, 0, 0, 0x80, 0xff, 0xbf]
    );
}

#[test]
fn noncanonical_real_zero_and_extended_negative_zero_become_canonical_zero() {
    let real = Real48::from_le_bytes([0, 1, 2, 3, 4, 0x80]);
    let extended = Extended80::from_le_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0x80]);
    assert_eq!(real.load_extended(), Extended80::ZERO);
    assert_eq!(extended.store_real(), Ok(Real48::ZERO));
}
