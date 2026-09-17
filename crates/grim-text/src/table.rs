//! Row tables: a header, a dash separator, and left/right-aligned rows.
//!
//! The caller passes final display strings, already escaped
//! (`grim_color::escape_codes`) — widths are byte lengths, so colour markup
//! would misalign. Sibling to [`crate::column_grid`], which packs keywords
//! rather than aligning heterogeneous rows.

/// Per-column alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    /// Pad with trailing spaces.
    #[default]
    Left,
    /// Pad with leading spaces.
    Right,
}

/// Render `headers` + dash separator + `rows` as an aligned table. Columns are
/// joined by two spaces with no trailing whitespace (trailing pad on the last
/// column is stripped; its leading pad for [`Align::Right`] survives).
///
/// `headers` sets the column count: short rows pad with `""`, long rows
/// truncate, so malformed data renders instead of panicking. Missing `align`
/// entries default to [`Align::Left`]. Zero columns → `""`; zero rows → the
/// header line alone.
pub fn table(headers: Vec<String>, rows: Vec<Vec<String>>, align: Vec<Align>) -> String {
    let cols = headers.len();
    if cols == 0 {
        return String::new();
    }
    let rows: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            let mut cells = row.clone();
            cells.truncate(cols);
            while cells.len() < cols {
                cells.push(String::new());
            }
            cells
        })
        .collect();
    let mut widths: Vec<usize> = headers.iter().map(String::len).collect();
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.len());
        }
    }
    let render_row = |cells: &[String]| -> String {
        cells
            .iter()
            .enumerate()
            .map(|(i, cell)| {
                let pad = widths[i].saturating_sub(cell.len());
                let aligned = match align.get(i).copied().unwrap_or(Align::Left) {
                    Align::Left => format!("{cell}{}", " ".repeat(pad)),
                    Align::Right => format!("{}{cell}", " ".repeat(pad)),
                };
                if i == cols - 1 {
                    aligned.trim_end().to_string()
                } else {
                    aligned
                }
            })
            .collect::<Vec<_>>()
            .join("  ")
    };
    let mut out = render_row(&headers);
    out.push('\n');
    if rows.is_empty() {
        return out;
    }
    let dashes: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    out.push_str(&render_row(&dashes));
    out.push('\n');
    for row in &rows {
        out.push_str(&render_row(row));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn left(n: usize) -> Vec<Align> {
        vec![Align::Left; n]
    }

    #[test]
    fn aligns_mixed_columns_with_separator() {
        let got = table(
            vec!["ID".into(), "IP".into(), "Idle".into()],
            vec![
                vec!["1".into(), "127.0.0.1:40001".into(), "12s".into()],
                vec!["22".into(), "10.0.0.8:22".into(), "184s".into()],
            ],
            vec![Align::Right, Align::Left, Align::Right],
        );
        let want = concat!(
            "ID  IP               Idle\n",
            "--  ---------------  ----\n",
            " 1  127.0.0.1:40001   12s\n",
            "22  10.0.0.8:22      184s\n",
        );
        assert_eq!(got, want);
    }

    #[test]
    fn short_rows_pad_and_long_rows_truncate() {
        let got = table(
            vec!["A".into(), "B".into()],
            vec![vec!["x".into()], vec!["1".into(), "2".into(), "3".into()]],
            left(2),
        );
        assert_eq!(got, "A  B\n-  -\nx  \n1  2\n");
    }

    #[test]
    fn missing_align_defaults_left_and_empty_cases() {
        let got = table(
            vec!["A".into(), "BB".into()],
            vec![vec!["1".into(), "2".into()]],
            vec![],
        );
        assert_eq!(got, "A  BB\n-  --\n1  2\n");
        assert_eq!(table(vec![], vec![vec!["x".into()]], vec![]), "");
        assert_eq!(table(vec!["A".into()], vec![], vec![]), "A\n");
    }
}
