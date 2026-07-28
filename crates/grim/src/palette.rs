/// Palette values for 16-color terminal markup codes.
///
/// Dark colors use nibble 0x9 = 153 (~60% of 255).
/// Bright colors use nibble 0xf = 255.
/// Greys/blacks are hand-tuned.
pub const BLACK_DARK: &str = "@x444";   // #444444  dark grey
pub const BLACK_BRIGHT: &str = "@x777"; // #777777  medium grey
pub const RED_DARK: &str = "@x900";     // #990000
pub const RED_BRIGHT: &str = "@xf00";   // #ff0000
pub const GREEN_DARK: &str = "@x090";   // #009900
pub const GREEN_BRIGHT: &str = "@x0f0"; // #00ff00
pub const YELLOW_DARK: &str = "@x990";  // #999900
pub const YELLOW_BRIGHT: &str = "@xff0"; // #ffff00
pub const BLUE_DARK: &str = "@x05a";    // #0055aa  dark blue
pub const BLUE_BRIGHT: &str = "@x07f";  // #0077ff  bright blue
pub const MAGENTA_DARK: &str = "@x909"; // #990099
pub const MAGENTA_BRIGHT: &str = "@xf0f"; // #ff00ff
pub const CYAN_DARK: &str = "@x099";    // #009999
pub const CYAN_BRIGHT: &str = "@x0ff";  // #00ffff
pub const WHITE_DARK: &str = "@xaaa";   // #aaaaaa  light grey
pub const WHITE_BRIGHT: &str = "@xfff"; // #ffffff
pub const RESET: &str = "@r";           // reset / default