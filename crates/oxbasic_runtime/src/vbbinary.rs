//! VB's binary layout for values in random and binary files. Each value is
//! written as VB writes it, so a file written here can be read by VB and the
//! other way round:
//!
//! - `Integer` 2 bytes, `Long` 4 bytes, little-endian two's complement.
//! - `Single` 4 bytes and `Double` 8 bytes, IEEE 754, little-endian.
//! - `Currency` 8 bytes: the amount times 10,000, as a little-endian integer.
//! - `Date` 8 bytes: a `Double` day serial.
//! - `Boolean` 2 bytes: `-1` for True, `0` for False.
//! - A dynamic `String` is a 2-byte length then its characters (one byte each).
//! - A fixed-length `String * n` is exactly `n` bytes, padded with spaces.

use crate::{err, ExecResult};

/// The VB type of one value in a file record, with what it needs to be
/// written or read: the size of a fixed string.
#[derive(Clone, Debug, PartialEq)]
pub enum FileType {
    Integer,
    Long,
    Single,
    Double,
    Currency,
    Date,
    Boolean,
    /// A dynamic string: length-prefixed.
    String,
    /// A fixed-length string of this many bytes.
    FixedString(usize),
}

/// Encode `v` as `ty` in VB's binary layout. A value of the wrong kind is
/// converted, as VB converts it on assignment.
pub fn encode_value(ty: &FileType, v: &crate::Variant) -> ExecResult<Vec<u8>> {
    use crate::Variant;
    let num = || -> ExecResult<f64> { v.as_f64() };
    Ok(match ty {
        FileType::Integer => {
            let x = v.as_i64()?;
            (x.clamp(i16::MIN as i64, i16::MAX as i64) as i16).to_le_bytes().to_vec()
        }
        FileType::Long => {
            let x = v.as_i64()?;
            (x.clamp(i32::MIN as i64, i32::MAX as i64) as i32).to_le_bytes().to_vec()
        }
        FileType::Single => (num()? as f32).to_le_bytes().to_vec(),
        FileType::Double | FileType::Date => num()?.to_le_bytes().to_vec(),
        FileType::Currency => ((num()? * 10_000.0).round() as i64).to_le_bytes().to_vec(),
        FileType::Boolean => {
            let b = v.as_bool()?;
            (if b { -1i16 } else { 0i16 }).to_le_bytes().to_vec()
        }
        FileType::String => {
            let s = match v {
                Variant::String(s) => s.clone(),
                other => other.as_string()?,
            };
            let bytes = s.as_bytes();
            let n = bytes.len().min(u16::MAX as usize);
            let mut out = (n as u16).to_le_bytes().to_vec();
            out.extend_from_slice(&bytes[..n]);
            out
        }
        FileType::FixedString(size) => {
            let s = match v {
                Variant::String(s) => s.clone(),
                other => other.as_string()?,
            };
            let mut out = vec![b' '; *size];
            let bytes = s.as_bytes();
            let k = bytes.len().min(*size);
            out[..k].copy_from_slice(&bytes[..k]);
            out
        }
    })
}

/// Decode one value of `ty` from `bytes` at `*at`, advancing `*at`. Returns
/// the value as the matching `Variant`.
pub fn decode_value(ty: &FileType, bytes: &[u8], at: &mut usize) -> ExecResult<crate::Variant> {
    use crate::Variant;
    let take = |n: usize, at: &mut usize| -> ExecResult<&[u8]> {
        if *at + n > bytes.len() {
            return err(62, "Input past end of file");
        }
        let s = &bytes[*at..*at + n];
        *at += n;
        Ok(s)
    };
    Ok(match ty {
        FileType::Integer => {
            let b = take(2, at)?;
            Variant::Integer(i16::from_le_bytes([b[0], b[1]]))
        }
        FileType::Long => {
            let b = take(4, at)?;
            Variant::Long(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        }
        FileType::Single => {
            let b = take(4, at)?;
            Variant::Single(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        }
        FileType::Double | FileType::Date => {
            let b = take(8, at)?;
            let x = f64::from_le_bytes(b.try_into().unwrap());
            if matches!(ty, FileType::Date) {
                Variant::Date(x)
            } else {
                Variant::Double(x)
            }
        }
        FileType::Currency => {
            let b = take(8, at)?;
            let x = i64::from_le_bytes(b.try_into().unwrap());
            Variant::Currency(x as f64 / 10_000.0)
        }
        FileType::Boolean => {
            let b = take(2, at)?;
            Variant::Boolean(i16::from_le_bytes([b[0], b[1]]) != 0)
        }
        FileType::String => {
            let len = {
                let b = take(2, at)?;
                u16::from_le_bytes([b[0], b[1]]) as usize
            };
            let b = take(len, at)?;
            Variant::String(String::from_utf8_lossy(b).into_owned())
        }
        FileType::FixedString(size) => {
            let b = take(*size, at)?;
            // a fixed string keeps its spaces in the file; VB trims nothing
            Variant::String(String::from_utf8_lossy(b).into_owned())
        }
    })
}
