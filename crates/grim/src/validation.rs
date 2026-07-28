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

/// Validate and normalize a character name.
/// Alphanumeric + spaces/hyphens/apostrophes, 3-20 chars, must start with a letter.
/// Returns normalized (trimmed, title-cased).
pub fn validate_character_name(input: &str) -> Result<String, ValidationError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::Empty);
    }
    if trimmed.len() < 3 {
        return Err(ValidationError::TooShort(3));
    }
    if trimmed.len() > 20 {
        return Err(ValidationError::TooLong(20));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '\'')
    {
        return Err(ValidationError::InvalidCharacters);
    }
    if !trimmed.chars().next().is_some_and(|c| c.is_alphabetic()) {
        return Err(ValidationError::InvalidFormat);
    }
    Ok(title_case(trimmed))
}

fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first
                    .to_uppercase()
                    .chain(chars.flat_map(|c| c.to_lowercase()))
                    .collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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
        assert_eq!(
            validate_identifier(""),
            Err(ValidationError::Empty)
        );
    }

    #[test]
    fn validate_identifier_whitespace() {
        assert_eq!(
            validate_identifier("   "),
            Err(ValidationError::Empty)
        );
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
        assert_eq!(
            validate_character_name("Aragorn").unwrap(),
            "Aragorn"
        );
    }

    #[test]
    fn validate_character_name_min_length() {
        assert_eq!(
            validate_character_name("Ara").unwrap(),
            "Ara"
        );
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
    fn validate_character_name_too_long() {
        let name = "Aaaaaaaaaaaaaaaaaaaaa"; // 21 chars
        assert_eq!(name.len(), 21);
        assert_eq!(
            validate_character_name(name),
            Err(ValidationError::TooLong(20))
        );
    }

    #[test]
    fn validate_character_name_starts_with_non_letter() {
        assert_eq!(
            validate_character_name("1aragorn"),
            Err(ValidationError::InvalidFormat)
        );
    }

    #[test]
    fn validate_character_name_starts_with_hyphen() {
        assert_eq!(
            validate_character_name("-aragorn"),
            Err(ValidationError::InvalidFormat)
        );
    }

    #[test]
    fn validate_character_name_invalid_chars() {
        assert_eq!(
            validate_character_name("Aragorn!"),
            Err(ValidationError::InvalidCharacters)
        );
    }

    #[test]
    fn validate_character_name_contains_space() {
        assert_eq!(
            validate_character_name("Aragorn II").unwrap(),
            "Aragorn Ii"
        );
    }

    #[test]
    fn validate_character_name_contains_hyphen() {
        assert_eq!(
            validate_character_name("Aragorn-II").unwrap(),
            "Aragorn-ii"
        );
    }

    #[test]
    fn validate_character_name_contains_apostrophe() {
        assert_eq!(
            validate_character_name("O'Brian").unwrap(),
            "O'brian"
        );
    }

    #[test]
    fn validate_character_name_title_casing() {
        assert_eq!(
            validate_character_name("aragorn the great").unwrap(),
            "Aragorn The Great"
        );
    }

    #[test]
    fn validate_character_name_title_casing_mixed() {
        assert_eq!(
            validate_character_name("aRAGORN").unwrap(),
            "Aragorn"
        );
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
        assert_eq!(
            normalize_filename("hello---world"),
            "hello---world"
        );
    }

    #[test]
    fn normalize_filename_leading_separators_trimmed() {
        assert_eq!(
            normalize_filename("__hello world__"),
            "hello_world"
        );
    }

    #[test]
    fn normalize_filename_trailing_separators_trimmed() {
        assert_eq!(
            normalize_filename("hello_world__"),
            "hello_world"
        );
    }

    #[test]
    fn normalize_filename_leading_and_trailing_trimmed() {
        assert_eq!(
            normalize_filename("  ___hello world___  "),
            "hello_world"
        );
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
        assert_eq!(
            validate_password(""),
            Err(ValidationError::Empty)
        );
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
        assert_eq!(ValidationError::TooShort(3).to_string(), "must be at least 3 characters");
        assert_eq!(ValidationError::TooLong(20).to_string(), "must be at most 20 characters");
        assert_eq!(ValidationError::InvalidCharacters.to_string(), "contains invalid characters");
        assert_eq!(ValidationError::InvalidEmail.to_string(), "is not a valid email address");
        assert_eq!(ValidationError::InvalidFormat.to_string(), "has an invalid format");
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
        assert_eq!(
            validate_character_name(""),
            Err(ValidationError::Empty)
        );
        assert_eq!(
            validate_character_name("   "),
            Err(ValidationError::Empty)
        );
    }

    // ── title_case: empty / whitespace-only input ──

    #[test]
    fn test_title_case_empty_word() {
        assert_eq!(title_case(""), "");
        assert_eq!(title_case("   "), "");
    }
}
