//! Find and replace for the code editor: the matches of a search text in
//! the lines, moving between them, and replacing them. Like the rest of the
//! text model it has no GUI in it, so it is tested headless. Positions are
//! the editor's `(row, column)` in characters.

use super::editor::{self, Pos, Range};

/// What to look for and how.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Search {
    pub text: String,
    /// Whether `Print` and `PRINT` are different words.
    pub match_case: bool,
}

/// Every place `search` occurs in `lines`, in reading order, without
/// overlaps (`aa` in `aaa` is found once, then from its end).
pub fn matches(lines: &[String], search: &Search) -> Vec<Range> {
    let fold = |c: char| {
        if search.match_case {
            c
        } else {
            c.to_lowercase().next().unwrap_or(c)
        }
    };
    let needle: Vec<char> = search.text.chars().map(fold).collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        let hay: Vec<char> = line.chars().map(fold).collect();
        let mut col = 0;
        while col + needle.len() <= hay.len() {
            if hay[col..col + needle.len()] == needle[..] {
                out.push(Range::between((row, col), (row, col + needle.len())));
                col += needle.len();
            } else {
                col += 1;
            }
        }
    }
    out
}

/// The first match that starts at or after `from`, wrapping to the first
/// match of the text when there is none after it. `None` when nothing matches.
pub fn next_from(found: &[Range], from: Pos) -> Option<usize> {
    if found.is_empty() {
        return None;
    }
    Some(found.iter().position(|r| r.start >= from).unwrap_or(0))
}

/// The last match that starts before `from`, wrapping to the last match when
/// there is none before it.
pub fn prev_from(found: &[Range], from: Pos) -> Option<usize> {
    if found.is_empty() {
        return None;
    }
    Some(
        found
            .iter()
            .rposition(|r| r.start < from)
            .unwrap_or(found.len() - 1),
    )
}

/// Replace the match `range` with `text`; returns where the replacement
/// ends, to search on from there.
pub fn replace_one(lines: &mut Vec<String>, range: Range, text: &str) -> Pos {
    editor::delete(lines, range);
    editor::insert(lines, range.start, text)
}

/// Replace every match of `search` with `text`, from the last to the first
/// so the earlier positions stay valid. Returns how many were replaced.
pub fn replace_all(lines: &mut Vec<String>, search: &Search, text: &str) -> usize {
    let found = matches(lines, search);
    for r in found.iter().rev() {
        replace_one(lines, *r, text);
    }
    found.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<String> {
        s.split('\n').map(|l| l.to_string()).collect()
    }

    fn s(text: &str, match_case: bool) -> Search {
        Search {
            text: text.into(),
            match_case,
        }
    }

    #[test]
    fn finds_every_match_in_reading_order_across_lines() {
        let l = lines("Print a\nprint b\nno\nPRINT");
        let found = matches(&l, &s("print", false));
        let starts: Vec<Pos> = found.iter().map(|r| r.start).collect();
        assert_eq!(starts, [(0, 0), (1, 0), (3, 0)]);
        assert_eq!(found[0].end, (0, 5), "the match is as long as the text");
    }

    #[test]
    fn match_case_tells_the_cases_apart() {
        let l = lines("Print print PRINT");
        assert_eq!(matches(&l, &s("print", true)).len(), 1);
        assert_eq!(matches(&l, &s("print", false)).len(), 3);
    }

    #[test]
    fn matches_do_not_overlap() {
        let found = matches(&lines("aaaa"), &s("aa", false));
        let starts: Vec<Pos> = found.iter().map(|r| r.start).collect();
        assert_eq!(starts, [(0, 0), (0, 2)]);
    }

    #[test]
    fn an_empty_search_or_no_match_finds_nothing() {
        let l = lines("abc");
        assert!(matches(&l, &s("", false)).is_empty());
        assert!(matches(&l, &s("zz", false)).is_empty());
        assert!(
            matches(&l, &s("abcd", false)).is_empty(),
            "longer than the text"
        );
    }

    #[test]
    fn columns_count_characters_not_bytes() {
        let found = matches(&lines("é x é"), &s("x", false));
        assert_eq!(found[0].start, (0, 2));
    }

    #[test]
    fn next_and_previous_wrap_round_the_text() {
        let found = matches(&lines("a a a"), &s("a", false));
        assert_eq!(next_from(&found, (0, 0)), Some(0), "from the first one");
        assert_eq!(next_from(&found, (0, 1)), Some(1));
        assert_eq!(next_from(&found, (0, 4)), Some(2));
        assert_eq!(next_from(&found, (0, 5)), Some(0), "wraps past the last");
        assert_eq!(prev_from(&found, (0, 4)), Some(1));
        assert_eq!(prev_from(&found, (0, 0)), Some(2), "wraps before the first");
        assert_eq!(next_from(&[], (0, 0)), None);
        assert_eq!(prev_from(&[], (0, 0)), None);
    }

    #[test]
    fn replace_one_puts_the_text_in_the_match() {
        let mut l = lines("Print x");
        let found = matches(&l, &s("print", false));
        let after = replace_one(&mut l, found[0], "Debug.Print");
        assert_eq!(l, lines("Debug.Print x"));
        assert_eq!(
            after,
            (0, 11),
            "the search goes on from the replacement's end"
        );
    }

    #[test]
    fn replace_all_changes_every_match_and_counts_them() {
        let mut l = lines("cat, concat\ncat");
        assert_eq!(replace_all(&mut l, &s("cat", true), "dog"), 3);
        assert_eq!(l, lines("dog, condog\ndog"));
    }

    #[test]
    fn replacing_with_a_longer_or_shorter_text_keeps_the_rest() {
        let mut l = lines("a-b-a");
        replace_all(&mut l, &s("a", true), "xyz");
        assert_eq!(l, lines("xyz-b-xyz"));
        replace_all(&mut l, &s("xyz", true), "");
        assert_eq!(l, lines("-b-"));
    }

    #[test]
    fn replacing_nothing_changes_nothing() {
        let mut l = lines("keep");
        assert_eq!(replace_all(&mut l, &s("zz", false), "y"), 0);
        assert_eq!(l, lines("keep"));
    }
}
