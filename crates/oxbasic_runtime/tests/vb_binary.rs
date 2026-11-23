//! Unit tests for VB's binary layout: each type's exact bytes.

use oxbasic_runtime::Variant;

fn enc(ty: &oxbasic_runtime::FileType, v: Variant) -> Vec<u8> {
    oxbasic_runtime::encode_value(ty, &v).unwrap()
}

#[test]
fn integer_is_two_bytes_little_endian() {
    assert_eq!(enc(&oxbasic_runtime::FileType::Integer, Variant::Long(258)), vec![2, 1]);
    assert_eq!(enc(&oxbasic_runtime::FileType::Integer, Variant::Long(-1)), vec![255, 255]);
}

#[test]
fn long_is_four_bytes_little_endian() {
    assert_eq!(
        enc(&oxbasic_runtime::FileType::Long, Variant::Long(65536)),
        vec![0, 0, 1, 0]
    );
}

#[test]
fn single_and_double_are_ieee_little_endian() {
    assert_eq!(
        enc(&oxbasic_runtime::FileType::Single, Variant::Double(1.0)),
        1.0f32.to_le_bytes().to_vec()
    );
    assert_eq!(
        enc(&oxbasic_runtime::FileType::Double, Variant::Double(1.5)),
        1.5f64.to_le_bytes().to_vec()
    );
}

#[test]
fn currency_is_the_amount_times_ten_thousand() {
    assert_eq!(
        enc(&oxbasic_runtime::FileType::Currency, Variant::Double(1.5)),
        15000i64.to_le_bytes().to_vec()
    );
}

#[test]
fn boolean_is_minus_one_for_true_and_zero_for_false() {
    assert_eq!(enc(&oxbasic_runtime::FileType::Boolean, Variant::Boolean(true)), vec![255, 255]);
    assert_eq!(enc(&oxbasic_runtime::FileType::Boolean, Variant::Boolean(false)), vec![0, 0]);
}

#[test]
fn dynamic_string_is_length_prefixed() {
    assert_eq!(
        enc(&oxbasic_runtime::FileType::String, Variant::String("ab".into())),
        vec![2, 0, b'a', b'b']
    );
}

#[test]
fn fixed_string_is_padded_with_spaces_to_its_size() {
    assert_eq!(
        enc(&oxbasic_runtime::FileType::FixedString(4), Variant::String("ab".into())),
        b"ab  ".to_vec()
    );
}

#[test]
fn every_type_decodes_back_to_its_value() {
    use oxbasic_runtime::FileType as T;
    let cases: Vec<(T, Variant)> = vec![
        (T::Integer, Variant::Integer(-7)),
        (T::Long, Variant::Long(123456)),
        (T::Single, Variant::Single(2.5)),
        (T::Double, Variant::Double(3.25)),
        (T::Currency, Variant::Currency(12.5)),
        (T::Date, Variant::Date(45000.0)),
        (T::Boolean, Variant::Boolean(true)),
        (T::String, Variant::String("hello".into())),
        (T::FixedString(6), Variant::String("abc   ".into())),
    ];
    for (ty, v) in cases {
        let bytes = oxbasic_runtime::encode_value(&ty, &v).unwrap();
        let mut at = 0;
        let back = oxbasic_runtime::decode_value(&ty, &bytes, &mut at).unwrap();
        assert_eq!(at, bytes.len(), "{:?} consumed all its bytes", ty);
        assert_eq!(format!("{:?}", back), format!("{:?}", v), "{:?}", ty);
    }
}
