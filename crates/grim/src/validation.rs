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
