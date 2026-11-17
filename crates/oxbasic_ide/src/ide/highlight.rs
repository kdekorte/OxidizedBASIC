//! BASIC syntax highlighting tokenizer for the IDE code editor.
//!
//! Scans a source line into colored spans: keywords, strings, numbers,
//! comments, identifiers, and punctuation.

/// Syntax token kinds, mapped to colors by the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tok {
    Keyword,
    Str,
    Num,
    Comment,
    Ident,
    Punct,
}

const KEYWORDS: &[&str] = &[
    "if", "then", "else", "elseif", "end", "select", "case", "for", "each", "in", "to",
    "step", "next", "do", "loop", "while", "until", "wend", "sub", "function", "exit",
    "dim", "static", "public", "private", "global", "const", "set", "call", "goto", "on",
    "error", "resume", "option", "explicit", "mod", "and", "or", "not", "xor", "eqv",
    "imp", "is", "like", "as", "new", "byval", "byref", "optional", "paramarray",
    "true", "false", "empty", "null", "nothing", "print", "rem",
];

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Tokenize one source line into (text, kind) spans.
pub fn tokenize_line(line: &str) -> Vec<(String, Tok)> {
    let mut out = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let rest: String = chars[i..].iter().collect();
        // comment: apostrophe anywhere, or Rem at a word boundary
        if chars[i] == '\'' {
            out.push((rest, Tok::Comment));
            break;
        }
        if chars[i].is_ascii_alphabetic()
            && chars[i].to_ascii_lowercase() == 'r'
            && chars.len() >= i + 3
            && chars[i + 1].to_ascii_lowercase() == 'e'
            && chars[i + 2].to_ascii_lowercase() == 'm'
            && (i == 0 || !is_ident_char(chars[i - 1]))
            && (i + 3 >= chars.len() || !is_ident_char(chars[i + 3]))
        {
            out.push((rest, Tok::Comment));
            break;
        }
        if chars[i] == '"' {
            let mut j = i + 1;
            while j < chars.len() {
                if chars[j] == '"' {
                    if j + 1 < chars.len() && chars[j + 1] == '"' {
                        j += 2; // escaped ""
                        continue;
                    }
                    j += 1;
                    break;
                }
                j += 1;
            }
            let s: String = chars[i..j.min(chars.len())].iter().collect();
            out.push((s, Tok::Str));
            i = j.min(chars.len());
            continue;
        }
        if chars[i].is_ascii_digit()
            || (chars[i] == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())
        {
            let mut j = i;
            while j < chars.len()
                && (chars[j].is_ascii_digit() || chars[j] == '.')
            {
                j += 1;
            }
            // exponent / hex suffix
            if j < chars.len() && (chars[j] == 'e' || chars[j] == 'E') {
                j += 1;
                if j < chars.len() && (chars[j] == '+' || chars[j] == '-') {
                    j += 1;
                }
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
            }
            let s: String = chars[i..j].iter().collect();
            out.push((s, Tok::Num));
            i = j;
            continue;
        }
        if chars[i].is_ascii_alphabetic() || chars[i] == '_' {
            let mut j = i;
            while j < chars.len() && is_ident_char(chars[j]) {
                j += 1;
            }
            let word: String = chars[i..j].iter().collect();
            let kind = if KEYWORDS.contains(&word.to_ascii_lowercase().as_str()) {
                Tok::Keyword
            } else {
                Tok::Ident
            };
            out.push((word, kind));
            i = j;
            continue;
        }
        // punctuation / whitespace runs
        let mut j = i;
        while j < chars.len()
            && !chars[j].is_ascii_alphanumeric()
            && chars[j] != '_'
            && chars[j] != '"'
            && chars[j] != '\''
        {
            j += 1;
        }
        let s: String = chars[i..j.max(i + 1)].iter().collect();
        out.push((s, Tok::Punct));
        i = j.max(i + 1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_a_line() {
        let toks = tokenize_line("If x >= 10 Then Print \"hi\" ' note");
        let kinds: Vec<Tok> = toks.iter().map(|(_, k)| *k).collect();
        assert_eq!(kinds[0], Tok::Keyword); // If
        let joined: String = toks.iter().map(|(t, _)| t.clone()).collect();
        assert_eq!(joined, "If x >= 10 Then Print \"hi\" ' note");
        assert!(kinds.contains(&Tok::Str));
        assert_eq!(*kinds.last().unwrap(), Tok::Comment);
    }

    #[test]
    fn rem_comment_and_numbers() {
        let toks = tokenize_line("Rem old");
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].1, Tok::Comment);
        let toks = tokenize_line("x = 3.5");
        assert!(toks.iter().any(|(t, k)| *k == Tok::Num && t == "3.5"));
    }
}
