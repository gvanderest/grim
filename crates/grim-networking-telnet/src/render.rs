//! ANSI render path for outbound telnet text: optional leading newline for
//! unsolicited events, the in-game `> ` prompt suffix (`<AFK> ` while the
//! session is flagged AFK), colour-code conversion, and `\n` → `\r\n`
//! line-ending translation.

use grim_color::ansi;

/// Turn a game output line into the exact string of bytes telnet expects:
/// prepend a newline for unsolicited events (so they don't land on the prompt
/// line), append an in-game prompt, render colour codes to ANSI, and
/// translate every `\n` to `\r\n`.
/// Legacy prompt path (`None` status). Test-only now — production always
/// resolves status first and calls [`render_output_with_status`] directly.
#[cfg(test)]
pub(crate) fn render_output(
    text: &str,
    is_ingame: bool,
    is_afk: bool,
    prepend_newline: bool,
) -> String {
    render_output_with_status(text, is_ingame, is_afk, prepend_newline, None)
}

/// Render with an optional live status prompt (`None` keeps the legacy
/// `> `). `Some((hp, max_hp, coin))` renders `"{hp}/{max} hp, {coin} coin >
/// "` (AFK still wins). Status is transport framing, not game text: no
/// catalog key, no colour codes — plain numbers the client cannot inject
/// into (formatted with `{}` from `u32`, never interpolated markup).
pub(crate) fn render_output_with_status(
    text: &str,
    is_ingame: bool,
    is_afk: bool,
    prepend_newline: bool,
    status: Option<(u32, u32, u32)>,
) -> String {
    // Prepend a newline for unsolicited events so they don't appear on the prompt line.
    let mut text = text.to_string();
    if prepend_newline && !text.is_empty() {
        text.insert(0, '\n');
    }
    let prompt = if is_afk {
        "<AFK> ".to_string()
    } else if let Some((hp, max_hp, coin)) = status {
        format!("{hp}/{max_hp} hp, {coin} coin > ")
    } else {
        "> ".to_string()
    };
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

    #[test]
    fn status_prompt_shows_hp_and_coin() {
        assert_eq!(
            render_output_with_status("hi", true, false, false, Some((87, 100, 45))),
            "hi\r\n87/100 hp, 45 coin > "
        );
        // AFK still wins over status.
        assert_eq!(
            render_output_with_status("hi", true, true, false, Some((87, 100, 45))),
            "hi\r\n<AFK> "
        );
        // No status keeps the legacy prompt.
        assert_eq!(
            render_output_with_status("hi", true, false, false, None),
            "hi\r\n> "
        );
    }
}
