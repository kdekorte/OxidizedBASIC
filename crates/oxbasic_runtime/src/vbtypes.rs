//! Mapping declared BASIC types to VB's file layouts.

use crate::vbbinary::FileType;

/// The `FileType` of a declared type name, as the runtime stores it (lower
/// case, with a fixed string's size after `*`: `string*10`). `None` for a
/// name that is not a built-in type (a user `Type` is laid out by its fields).
pub fn file_type_of(name: &str) -> Option<FileType> {
    let lower = name.trim().to_ascii_lowercase();
    if let Some((base, size)) = lower.split_once('*') {
        if base == "string" {
            return size.trim().parse::<usize>().ok().map(FileType::FixedString);
        }
        return None;
    }
    Some(match lower.as_str() {
        "integer" | "int" => FileType::Integer,
        "long" => FileType::Long,
        "single" => FileType::Single,
        "double" => FileType::Double,
        "currency" => FileType::Currency,
        "date" => FileType::Date,
        "boolean" => FileType::Boolean,
        "string" => FileType::String,
        _ => return None,
    })
}

/// The size of a fixed-layout value in a file, in bytes. A dynamic string is
/// length-prefixed and has no fixed size (`None`).
pub fn fixed_size(ty: &FileType) -> Option<usize> {
    match ty {
        FileType::Integer | FileType::Boolean => Some(2),
        FileType::Long | FileType::Single => Some(4),
        FileType::Double | FileType::Currency | FileType::Date => Some(8),
        FileType::String => None,
        FileType::FixedString(n) => Some(*n),
    }
}

/// Lay out one record of a user `Type` in VB's binary form: each field's
/// bytes in field order, with no padding between them. `fields` are the
/// declared fields `(name, type name)` in order; `values` the record's values
/// in the same order. A field that is itself a record is not laid out yet.
pub fn encode_record(
    fields: &[(String, String)],
    values: &[(String, crate::Variant)],
) -> crate::ExecResult<Vec<u8>> {
    let mut out = Vec::new();
    for (decl_name, decl_ty) in fields {
        let Some((_, v)) = values.iter().find(|(n, _)| n.eq_ignore_ascii_case(decl_name)) else {
            return crate::err(9, "record field missing");
        };
        let Some(ty) = file_type_of(decl_ty) else {
            return crate::err(5, "Type of a record field is not supported in a file yet");
        };
        out.extend(crate::encode_value(&ty, v)?);
    }
    Ok(out)
}
