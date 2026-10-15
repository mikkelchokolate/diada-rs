use diada_compat::{Extended80, Real48, multiply_real48};

fn hex<const N: usize>(text: &str) -> [u8; N] {
    assert_eq!(text.len(), N * 2);
    std::array::from_fn(|index| u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).unwrap())
}

#[test]
fn matches_measured_dosbox_auto_extended_products() {
    let mut count = 0;
    let mut software_differences = 0;
    for row in include_str!("../fixtures/multiply.tsv").lines().skip(1) {
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(fields.len(), 5, "malformed oracle row");
        let left = Real48::from_le_bytes(hex(fields[1]));
        let right = Real48::from_le_bytes(hex(fields[2]));
        let expected = Extended80::from_le_bytes(hex(fields[3]));
        assert_eq!(multiply_real48(left, right), expected, "{}", fields[0]);
        software_differences += usize::from(fields[3] != fields[4]);
        count += 1;
    }
    assert_eq!(count, 5330, "oracle corpus is incomplete");
    assert_eq!(
        software_differences, 372,
        "software-emulator comparison changed"
    );
}

#[test]
fn preserves_the_measured_sign_of_extended_zero() {
    let zero = Real48::ZERO;
    let encoded_negative_zero = Real48::from_le_bytes([0, 0xff, 0xff, 0xff, 0xff, 0x80]);
    let negative_one = Real48::from_le_bytes([129, 0, 0, 0, 0, 0x80]);
    let negative_extended_zero = Extended80::from_le_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0x80]);

    assert_eq!(multiply_real48(zero, negative_one), negative_extended_zero);
    assert_eq!(multiply_real48(negative_one, zero), negative_extended_zero);
    assert_eq!(
        multiply_real48(encoded_negative_zero, negative_one),
        negative_extended_zero
    );
    assert_eq!(
        multiply_real48(encoded_negative_zero, zero),
        Extended80::ZERO
    );
}

#[test]
fn keeps_the_extended_product_that_f64_double_rounding_changes() {
    let left = Real48::from_le_bytes(hex("81a029e74d1d"));
    let right = Real48::from_le_bytes(hex("817ae0d40555"));
    let product = multiply_real48(left, right);
    assert_eq!(product.to_le_bytes(), hex("16fd7f98a966e5820040"));
    assert_eq!(
        product.store_real().unwrap().to_le_bytes(),
        hex("8298a966e502")
    );
}
