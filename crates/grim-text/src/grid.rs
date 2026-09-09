//! Keyword grids: sort words, fill across each row, space every column
//! equally. No separators, no trailing whitespace — just keywords packed to
//! use the width (Waterdeep's `commands` shape at 80 columns).

/// Minimum column width; columns are never narrower than this even when
/// every word is short.
pub const MIN_COLUMN_WIDTH: usize = 7;

/// Lay `words` out in rows of as many equal columns as fit `max_width`,
/// filling across (`a b / c d`, not `a c / b d`). Words are sorted; every
/// column is as wide as the longest word (floored at [`MIN_COLUMN_WIDTH`])
/// plus one space of gap. A single over-long word still renders (one per
/// line) rather than being split.
pub fn column_grid(words: &[String], max_width: usize) -> String {
    if words.is_empty() {
        return String::new();
    }
    let mut sorted = words.to_vec();
    sorted.sort();
    let width = sorted
        .iter()
        .map(String::len)
        .max()
        .unwrap_or(0)
        .max(MIN_COLUMN_WIDTH);
    let columns = ((max_width.max(1) + 1) / (width + 1))
        .max(1)
        .min(sorted.len());
    let mut out = String::new();
    for chunk in sorted.chunks(columns) {
        let mut line = String::new();
        for (c, word) in chunk.iter().enumerate() {
            if c > 0 {
                line.push(' ');
            }
            line.push_str(word);
            line.extend(std::iter::repeat_n(' ', width.saturating_sub(word.len())));
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_across_equal_columns() {
        let words = ["a", "b", "c", "d", "e"].map(str::to_string);
        assert_eq!(column_grid(&words, 16), "a       b\nc       d\ne\n");
    }

    #[test]
    fn packs_as_many_columns_as_fit() {
        let words = ["alpha", "beta", "delta", "gamma", "epsilon"].map(str::to_string);
        // Sorted: alpha, beta, delta, epsilon, gamma.
        assert_eq!(
            column_grid(&words, 20),
            "alpha   beta\ndelta   epsilon\ngamma\n"
        );
    }

    #[test]
    fn sorts_before_layout() {
        let words = ["say", "grin", "look"].map(str::to_string);
        assert_eq!(column_grid(&words, 80), "grin    look    say\n");
    }

    #[test]
    fn columns_floor_at_minimum_width() {
        let words = ["a", "bb", "c", "dddd", "e"].map(str::to_string);
        // Longest is 4, floored to 7: width 10 fits one column only.
        assert_eq!(column_grid(&words, 10), "a\nbb\nc\ndddd\ne\n");
    }

    #[test]
    fn narrows_to_fit_the_width() {
        let words = ["alpha", "beta", "gamma", "delta"].map(str::to_string);
        // Width 12 fits one floored column only.
        assert_eq!(column_grid(&words, 12), "alpha\nbeta\ndelta\ngamma\n");
    }

    #[test]
    fn empty_is_empty_and_long_word_survives() {
        assert_eq!(column_grid(&[], 80), "");
        let words = ["supercalifragilistic".to_string()];
        assert_eq!(column_grid(&words, 5), "supercalifragilistic\n");
    }

    #[test]
    fn no_trailing_whitespace_on_short_rows() {
        let words = ["a", "b", "c", "dddd", "e", "ff"].map(str::to_string);
        let grid = column_grid(&words, 80);
        for line in grid.lines() {
            assert_eq!(line, line.trim_end(), "trailing space: {line:?}");
        }
    }
}
