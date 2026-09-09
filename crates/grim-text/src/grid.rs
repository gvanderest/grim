//! Keyword grids: sort words, fill across each row, size every column to its
//! longest word. No separators, no trailing whitespace — just keywords packed
//! to use the width (Waterdeep's `commands` shape at 80 columns).

/// Lay `words` out in rows of as many columns as fit `max_width`, filling
/// across (`a b / c d`, not `a c / b d`). Words are sorted; each column is as
/// wide as its longest word plus a two-space gap. A single over-long word
/// still renders (one per line) rather than being split.
pub fn column_grid(words: &[String], max_width: usize) -> String {
    if words.is_empty() {
        return String::new();
    }
    let mut sorted = words.to_vec();
    sorted.sort();
    let len = sorted.len();
    // Widest layout wins: try the most columns first, keep the first one
    // whose columns fit. One column always fits.
    for columns in (1..=len).rev() {
        let widths: Vec<usize> = (0..columns)
            .map(|c| {
                (c..len)
                    .step_by(columns)
                    .map(|i| sorted[i].len())
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        let total: usize = widths.iter().sum::<usize>() + columns.saturating_sub(1);
        if total <= max_width.max(1) {
            return render_grid(&sorted, columns, &widths);
        }
    }
    render_grid(
        &sorted,
        1,
        &[sorted.iter().map(String::len).max().unwrap_or(0)],
    )
}

fn render_grid(words: &[String], columns: usize, widths: &[usize]) -> String {
    let mut out = String::new();
    for chunk in words.chunks(columns) {
        let mut line = String::new();
        for (c, word) in chunk.iter().enumerate() {
            if c > 0 {
                line.push(' ');
            }
            line.push_str(word);
            line.extend(std::iter::repeat_n(
                ' ',
                widths[c].saturating_sub(word.len()),
            ));
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
    fn fills_across_rows() {
        let words = ["a", "b", "c", "d", "e"].map(str::to_string);
        assert_eq!(column_grid(&words, 6), "a b c\nd e\n");
    }

    #[test]
    fn packs_as_many_columns_as_fit() {
        let words = ["a", "b", "c", "d", "e"].map(str::to_string);
        assert_eq!(column_grid(&words, 10), "a b c d e\n");
    }

    #[test]
    fn sorts_before_layout() {
        let words = ["say", "grin", "look"].map(str::to_string);
        assert_eq!(column_grid(&words, 80), "grin look say\n");
    }

    #[test]
    fn columns_size_to_their_longest_word() {
        let words = ["a", "bb", "c", "dddd", "e"].map(str::to_string);
        // Three columns: col0=[a,dddd] w=4, col1=[bb,e] w=2, col2=[c] w=1.
        assert_eq!(column_grid(&words, 10), "a    bb c\ndddd e\n");
    }

    #[test]
    fn narrows_to_fit_the_width() {
        let words = ["alpha", "beta", "gamma", "delta"].map(str::to_string);
        assert_eq!(column_grid(&words, 12), "alpha beta\ndelta gamma\n");
    }

    #[test]
    fn empty_is_empty_and_long_word_survives() {
        assert_eq!(column_grid(&[], 80), "");
        let words = ["supercalifragilistic".to_string()];
        assert_eq!(column_grid(&words, 5), "supercalifragilistic\n");
    }

    #[test]
    fn no_trailing_whitespace_on_short_rows() {
        let words = ["a", "b", "c"].map(str::to_string);
        let grid = column_grid(&words, 80);
        for line in grid.lines() {
            assert_eq!(line, line.trim_end(), "trailing space: {line:?}");
        }
    }
}
