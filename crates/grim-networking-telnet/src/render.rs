//! ANSI render path for outbound telnet text: optional leading newline for
//! unsolicited events, the in-game `> ` prompt suffix (`<AFK> ` while the
//! session is flagged AFK), colour-code conversion, and `\n` → `\r\n`
//! line-ending translation.

use grim_color::ansi;

/// Turn a game output line into the exact string of bytes telnet expects:
/// prepend a newline for unsolicited events (so they don't land on the prompt
/// line), append an in-game prompt, render colour codes to ANSI, and
/// translate every `\n` to `\r\n`.
pub(crate) fn render_output(
    text: &str,
    is_ingame: bool,
    is_afk: bool,
    prepend_newline: bool,
) -> String {
    // Prepend a newline for unsolicited events so they don't appear on the prompt line.
    let mut text = text.to_string();
    if prepend_newline && !text.is_empty() {
        text.insert(0, '\n');
    }
    let prompt = if is_afk { "<AFK> " } else { "> " };
    let send_text = if is_ingame && !text.is_empty() {
        format!("{text}\n{prompt}")
    } else {
        text
    };
    let palette = grim_color::convert_16color(&send_text);
    let colored = ansi(&palette);
    colored.replace('\n', "\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn afk_prompt_replaces_default() {
        assert_eq!(render_output("hi", true, false, false), "hi\r\n> ");
        assert_eq!(render_output("hi", true, true, false), "hi\r\n<AFK> ");
        assert_eq!(render_output("hi", false, true, false), "hi");
    }
}
