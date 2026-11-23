use oxbasic_runtime::Variant;

#[test]
fn a_record_is_its_fields_in_order_with_no_padding() {
    let fields = vec![
        ("Name".to_string(), "string*4".to_string()),
        ("Age".to_string(), "integer".to_string()),
    ];
    let values = vec![
        ("Name".to_string(), Variant::String("Al".into())),
        ("Age".to_string(), Variant::Integer(258)),
    ];
    let bytes = oxbasic_runtime::encode_record(&fields, &values).unwrap();
    // "Al  " (4 bytes) then 258 as a 2-byte little-endian Integer
    assert_eq!(bytes, vec![b'A', b'l', b' ', b' ', 2, 1]);
}
