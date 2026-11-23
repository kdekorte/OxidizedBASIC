use oxbasic_runtime::{file_type_of, fixed_size, FileType};

#[test]
fn builtin_type_names_map_to_their_file_types() {
    assert_eq!(file_type_of("Integer"), Some(FileType::Integer));
    assert_eq!(file_type_of("long"), Some(FileType::Long));
    assert_eq!(file_type_of("Double"), Some(FileType::Double));
    assert_eq!(file_type_of("String"), Some(FileType::String));
}

#[test]
fn a_fixed_string_maps_to_its_declared_size() {
    assert_eq!(file_type_of("string*10"), Some(FileType::FixedString(10)));
}

#[test]
fn a_user_type_name_is_not_a_builtin_file_type() {
    assert_eq!(file_type_of("Customer"), None);
}

#[test]
fn fixed_sizes_match_vb() {
    assert_eq!(fixed_size(&FileType::Integer), Some(2));
    assert_eq!(fixed_size(&FileType::Long), Some(4));
    assert_eq!(fixed_size(&FileType::Double), Some(8));
    assert_eq!(fixed_size(&FileType::FixedString(7)), Some(7));
    assert_eq!(fixed_size(&FileType::String), None);
}
