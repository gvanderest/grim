use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    Empty,
    TooShort(usize),
    TooLong(usize),
    InvalidCharacters,
    InvalidEmail,
    InvalidFormat,
}

/// Default reserved name prefixes. A character name may not *begin* with any of
/// these (case-insensitive) so it can never be confused with a targeting keyword
/// — a command like `cast fire self` must unambiguously mean the caster, never a
/// character named "Selfius". Prefix-based on purpose: "Selfius" is rejected but
/// "Zself" is fine. This is only the DEFAULT list; the server exposes it as
/// configuration (see `grim_scene::ReservedNamePrefixes`) so an author can extend
/// or replace it. Enforcement lives at the creation boundary, not in the pure
/// validator, so the configured list is what actually applies.
pub const DEFAULT_RESERVED_NAME_PREFIXES: &[&str] = &["self", "someone"];

/// Whether `name` begins with any of `prefixes` (case-insensitive). Used at
/// character creation against the configured reserved list.
pub fn is_name_reserved(name: &str, prefixes: &[String]) -> bool {
    let lower = name.to_lowercase();
    prefixes
        .iter()
        .any(|p| lower.starts_with(&p.to_lowercase()))
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "cannot be empty"),
            Self::TooShort(min) => write!(f, "must be at least {min} characters"),
            Self::TooLong(max) => write!(f, "must be at most {max} characters"),
            Self::InvalidCharacters => write!(f, "contains invalid characters"),
            Self::InvalidEmail => write!(f, "is not a valid email address"),
            Self::InvalidFormat => write!(f, "has an invalid format"),
        }
    }
}

impl std::error::Error for ValidationError {}

/// Validate and normalize an email address for use as an account identifier.
/// Emails: must contain @ with text before and after, and a dot in the domain.
/// Returns normalized (lowercase, trimmed).
pub fn validate_identifier(input: &str) -> Result<String, ValidationError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::Empty);
    }
    let lower = trimmed.to_lowercase();
    validate_email(&lower)
}

fn validate_email(s: &str) -> Result<String, ValidationError> {
    let parts: Vec<&str> = s.split('@').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(ValidationError::InvalidEmail);
    }
    if !parts[1].contains('.') {
        return Err(ValidationError::InvalidEmail);
    }
    if parts[1].split('.').any(|p| p.is_empty()) {
        return Err(ValidationError::InvalidEmail);
    }
    Ok(s.to_string())
}

/// Validate a character name (the *input-validation* level). Names are ASCII
/// letters only, 3–20 of them: any non-letter — space, digit, hyphen, apostrophe
/// — is rejected, not silently removed (`"Wrack-"` is invalid; `"wrack"` is not).
/// On success returns the canonical form via [`normalize_character_name`].
///
/// The reserved-word check is deliberately NOT done here — it is enforced at the
/// character-creation boundary against the server's configured list
/// ([`is_name_reserved`] / [`crate::components::ReservedNamePrefixes`]) so a
/// custom list actually applies.
pub fn validate_character_name(input: &str) -> Result<String, ValidationError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::Empty);
    }
    // Charset: ASCII letters only. Reject anything else rather than stripping it,
    // so the player gets clear feedback on an invalid name.
    if !trimmed.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(ValidationError::InvalidCharacters);
    }
    let len = trimmed.chars().count();
    if len < 3 {
        return Err(ValidationError::TooShort(3));
    }
    if len > 20 {
        return Err(ValidationError::TooLong(20));
    }
    Ok(normalize_character_name(trimmed))
}

/// The canonical form of a character name (the *interpretation/storage* level):
/// capitalize the first letter and lowercase the rest, using ASCII casing so it
/// is idempotent and never changes length (`"wrack"` / `"WRACK"` / `"wRaCk"` →
/// `"Wrack"`). It does not strip — creation validates the charset first
/// ([`validate_character_name`]); this only canonicalizes case. It is also
/// applied to raw input at the login prompt so a typed name matches the stored
/// canonical form exactly.
pub fn normalize_character_name(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => {
            let mut out = String::with_capacity(s.len());
            out.push(first.to_ascii_uppercase());
            out.extend(chars.map(|c| c.to_ascii_lowercase()));
            out
        }
        None => String::new(),
    }
}

/// Normalize a string for use as a filename.
/// Lowercase, spaces → underscores, other non-alphanumeric → hyphens.
pub fn normalize_filename(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c
            } else if c == ' ' || c == '_' {
                '_'
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches(|c: char| c == '_' || c == '-')
        .to_string()
}

/// Validate a password. Minimum 6 characters.
pub fn validate_password(s: &str) -> Result<(), ValidationError> {
    if s.is_empty() {
        return Err(ValidationError::Empty);
    }
    if s.len() < 6 {
        return Err(ValidationError::TooShort(6));
    }
    Ok(())
}

/// Hash a password using SHA-256.
/// NOTE: PoC only — no salt, not slow. Replace with bcrypt/argon2 before production.
pub fn hash_password(password: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(password.as_bytes()))
}

/// Verify a password against a hash.
pub fn verify_password(password: &str, hash: &str) -> bool {
    hash_password(password) == hash
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── validate_identifier ──

    #[test]
    fn validate_identifier_valid_email() {
        assert_eq!(
            validate_identifier("User@Example.com").unwrap(),
            "user@example.com"
        );
    }

    #[test]
    fn validate_identifier_no_at() {
        assert_eq!(
            validate_identifier("userexample.com"),
            Err(ValidationError::InvalidEmail)
        );
    }

    #[test]
    fn validate_identifier_no_dot_in_domain() {
        assert_eq!(
            validate_identifier("user@example"),
            Err(ValidationError::InvalidEmail)
        );
    }

    #[test]
    fn validate_identifier_empty() {
        assert_eq!(validate_identifier(""), Err(ValidationError::Empty));
    }

    #[test]
    fn validate_identifier_whitespace() {
        assert_eq!(validate_identifier("   "), Err(ValidationError::Empty));
    }

    #[test]
    fn validate_identifier_trailing_spaces_trimmed() {
        assert_eq!(
            validate_identifier("  User@Example.com  ").unwrap(),
            "user@example.com"
        );
    }

    // ── validate_character_name ──

    #[test]
    fn validate_character_name_valid() {
        assert_eq!(validate_character_name("Aragorn").unwrap(), "Aragorn");
    }

    #[test]
    fn validate_character_name_min_length() {
        assert_eq!(validate_character_name("Ara").unwrap(), "Ara");
    }

    #[test]
    fn validate_character_name_max_length() {
        let name = "Aaaaaaaaaaaaaaaaaaaa"; // 20 chars, starts with letter
        assert_eq!(name.len(), 20);
        assert_eq!(
            validate_character_name(name).unwrap(),
            "Aaaaaaaaaaaaaaaaaaaa"
        );
    }

    #[test]
    fn validate_character_name_too_short() {
        assert_eq!(
            validate_character_name("Ab"),
            Err(ValidationError::TooShort(3))
        );
    }

    #[test]
    fn validate_character_name_empty() {
        assert_eq!(validate_character_name("   "), Err(ValidationError::Empty));
    }

    #[test]
    fn validate_character_name_too_long() {
        let name = "Aaaaaaaaaaaaaaaaaaaaa"; // 21 letters
        assert_eq!(name.len(), 21);
        assert_eq!(
            validate_character_name(name),
            Err(ValidationError::TooLong(20))
        );
    }

    // Non-letters are REJECTED, not stripped: "wrack" ok, "Wrack-" invalid.
    #[test]
    fn validate_character_name_rejects_non_letters() {
        for bad in [
            "Wrack-",
            "Aragorn II",
            "O'Brian",
            "Wr4ck",
            "1aragorn",
            "hero_",
        ] {
            assert_eq!(
                validate_character_name(bad),
                Err(ValidationError::InvalidCharacters),
                "{bad} should be rejected"
            );
        }
    }

    #[test]
    fn validate_character_name_normalizes_case_of_valid_input() {
        assert_eq!(validate_character_name("wrack").unwrap(), "Wrack");
        assert_eq!(validate_character_name("WRACK").unwrap(), "Wrack");
        assert_eq!(validate_character_name("wRaCk").unwrap(), "Wrack");
        assert_eq!(validate_character_name("  aragorn  ").unwrap(), "Aragorn");
    }

    #[test]
    fn normalize_character_name_is_idempotent() {
        for n in ["wrack", "WRACK", "wRaCk", "Aragorn"] {
            let once = normalize_character_name(n);
            assert_eq!(normalize_character_name(&once), once, "idempotent for {n}");
        }
    }

    #[test]
    fn is_name_reserved_is_prefix_based_and_case_insensitive() {
        let prefixes: Vec<String> = DEFAULT_RESERVED_NAME_PREFIXES
            .iter()
            .map(|s| s.to_string())
            .collect();
        for n in ["self", "Self", "Selfius", "someone", "SomeoneElse"] {
            assert!(is_name_reserved(n, &prefixes), "{n} should be reserved");
        }
        // A reserved word not at the start is fine.
        assert!(!is_name_reserved("Zself", &prefixes));
        assert!(!is_name_reserved("Bobself", &prefixes));
        // Empty configured list reserves nothing.
        assert!(!is_name_reserved("Self", &[]));
    }

    // ── normalize_filename ──

    #[test]
    fn normalize_filename_basic() {
        assert_eq!(normalize_filename("hello"), "hello");
    }

    #[test]
    fn normalize_filename_spaces() {
        assert_eq!(normalize_filename("hello world"), "hello_world");
    }

    #[test]
    fn normalize_filename_mixed_separators() {
        assert_eq!(
            normalize_filename("hello world foo_bar"),
            "hello_world_foo_bar"
        );
    }

    #[test]
    fn normalize_filename_special_chars() {
        // non-alphanumeric, non-space → hyphens
        assert_eq!(normalize_filename("hello!!!world"), "hello---world");
    }

    #[test]
    fn normalize_filename_multiple_special_chars() {
        assert_eq!(normalize_filename("hello---world"), "hello---world");
    }

    #[test]
    fn normalize_filename_leading_separators_trimmed() {
        assert_eq!(normalize_filename("__hello world__"), "hello_world");
    }

    #[test]
    fn normalize_filename_trailing_separators_trimmed() {
        assert_eq!(normalize_filename("hello_world__"), "hello_world");
    }

    #[test]
    fn normalize_filename_leading_and_trailing_trimmed() {
        assert_eq!(normalize_filename("  ___hello world___  "), "hello_world");
    }

    // ── validate_password ──

    #[test]
    fn validate_password_valid() {
        assert_eq!(validate_password("abc123"), Ok(()));
    }

    #[test]
    fn validate_password_exactly_six() {
        assert_eq!(validate_password("abcdef"), Ok(()));
    }

    #[test]
    fn validate_password_too_short() {
        assert_eq!(
            validate_password("abc12"),
            Err(ValidationError::TooShort(6))
        );
    }

    #[test]
    fn validate_password_empty() {
        assert_eq!(validate_password(""), Err(ValidationError::Empty));
    }

    // ── hash_password ──

    #[test]
    fn hash_password_returns_hex_string() {
        let hash = hash_password("hello");
        // SHA-256 hex = 64 chars
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hash_password_deterministic() {
        assert_eq!(hash_password("hello"), hash_password("hello"));
    }

    #[test]
    fn hash_password_differs_for_diff_inputs() {
        assert_ne!(hash_password("hello"), hash_password("world"));
    }

    // ── verify_password ──

    #[test]
    fn verify_password_matching() {
        let hash = hash_password("correct_password");
        assert!(verify_password("correct_password", &hash));
    }

    #[test]
    fn verify_password_non_matching() {
        let hash = hash_password("correct_password");
        assert!(!verify_password("wrong_password", &hash));
    }

    // ── ValidationError Display ──

    #[test]
    fn test_validation_error_display_variants() {
        assert_eq!(ValidationError::Empty.to_string(), "cannot be empty");
        assert_eq!(
            ValidationError::TooShort(3).to_string(),
            "must be at least 3 characters"
        );
        assert_eq!(
            ValidationError::TooLong(20).to_string(),
            "must be at most 20 characters"
        );
        assert_eq!(
            ValidationError::InvalidCharacters.to_string(),
            "contains invalid characters"
        );
        assert_eq!(
            ValidationError::InvalidEmail.to_string(),
            "is not a valid email address"
        );
        assert_eq!(
            ValidationError::InvalidFormat.to_string(),
            "has an invalid format"
        );
    }

    // ── validate_email: empty domain part ──

    #[test]
    fn test_validate_email_empty_domain_part() {
        assert_eq!(
            validate_identifier("a@b."),
            Err(ValidationError::InvalidEmail)
        );
        assert_eq!(
            validate_identifier("a@.b"),
            Err(ValidationError::InvalidEmail)
        );
    }

    // ── validate_character_name: empty / whitespace-only ──

    #[test]
    fn test_validate_character_name_empty() {
        assert_eq!(validate_character_name(""), Err(ValidationError::Empty));
        assert_eq!(validate_character_name("   "), Err(ValidationError::Empty));
    }

    // ── normalize_character_name ──

    #[test]
    fn normalize_character_name_titlecases_without_stripping() {
        // Case-only canonicalization; charset rejection is validate's job.
        assert_eq!(normalize_character_name(""), "");
        assert_eq!(normalize_character_name("aB"), "Ab");
        assert_eq!(normalize_character_name("WRACK"), "Wrack");
    }
}
