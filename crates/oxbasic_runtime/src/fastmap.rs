//! What variable lookup is built on: a fast hasher for the interpreter's name
//! tables, and a way to lowercase a name (BASIC names are case-insensitive)
//! without allocating. Profiling the Mandelbrot sample showed the time going
//! to `to_lowercase`, `malloc`/`free` and SipHash rather than to arithmetic.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// The Fx hash (as in rustc): a multiply and a rotate per word. The names
/// it hashes are short and not attacker-chosen, so SipHash's protection
/// against collisions is not worth its cost here.
#[derive(Default, Clone, Copy)]
pub(crate) struct FxHasher {
    hash: u64,
}

const K: u64 = 0x517c_c1b7_2722_0a95;

impl FxHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(K);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for c in &mut chunks {
            self.add(u64::from_le_bytes(c.try_into().unwrap()));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut b = [0u8; 8];
            b[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(b));
        }
    }

    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add(i as u64);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

/// A map keyed by (lowercased) name, with the fast hasher.
pub(crate) type NameMap<V> = HashMap<String, V, BuildHasherDefault<FxHasher>>;

/// A name in lowercase, kept on the stack when it is short ASCII (all but
/// the oddest identifiers), so looking it up costs no allocation. A name
/// that is already lowercase is borrowed as it is.
pub(crate) enum Lowered<'a> {
    Borrowed(&'a str),
    Stack([u8; 32], u8),
    Heap(String),
}

impl<'a> Lowered<'a> {
    #[inline]
    pub(crate) fn new(name: &'a str) -> Lowered<'a> {
        let b = name.as_bytes();
        if !b.iter().any(|c| c.is_ascii_uppercase()) && name.is_ascii() {
            return Lowered::Borrowed(name);
        }
        if b.len() <= 32 && name.is_ascii() {
            let mut stack = [0u8; 32];
            for (d, s) in stack.iter_mut().zip(b) {
                *d = s.to_ascii_lowercase();
            }
            Lowered::Stack(stack, b.len() as u8)
        } else {
            Lowered::Heap(name.to_lowercase())
        }
    }

    #[inline]
    pub(crate) fn as_str(&self) -> &str {
        match self {
            Lowered::Borrowed(s) => s,
            Lowered::Heap(s) => s,
            // SAFETY: the bytes are an ASCII `str`'s, lowercased: still ASCII
            Lowered::Stack(b, n) => unsafe { std::str::from_utf8_unchecked(&b[..*n as usize]) },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_lowercase_on_the_stack_or_the_heap() {
        assert_eq!(Lowered::new("ZR2").as_str(), "zr2");
        assert_eq!(Lowered::new("").as_str(), "");
        assert_eq!(Lowered::new("Straße").as_str(), "straße");
        let long = "A".repeat(100);
        assert_eq!(Lowered::new(&long).as_str(), "a".repeat(100));
    }

    #[test]
    fn the_name_map_finds_what_it_was_given() {
        let mut m: NameMap<usize> = NameMap::default();
        for (i, n) in ["x", "zr2", "a_long_variable_name", "ci"]
            .iter()
            .enumerate()
        {
            m.insert(n.to_string(), i);
        }
        assert_eq!(m.get("zr2"), Some(&1));
        assert_eq!(m.get("a_long_variable_name"), Some(&2));
        assert_eq!(m.get(Lowered::new("CI").as_str()), Some(&3));
        assert_eq!(m.get("nope"), None);
    }
}
