use std::fmt;

use serde::{Deserialize, Serialize};

/// The six cardinal directions used for room exits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Cardinal {
    North,
    East,
    South,
    West,
    Up,
    Down,
}

impl Cardinal {
    /// Parse a direction from a string (case-insensitive, supports abbreviations).
    pub fn parse(input: &str) -> Option<Self> {
        match input.to_lowercase().as_str() {
            "n" | "north" => Some(Self::North),
            "e" | "east" => Some(Self::East),
            "s" | "south" => Some(Self::South),
            "w" | "west" => Some(Self::West),
            "u" | "up" => Some(Self::Up),
            "d" | "down" => Some(Self::Down),
            _ => None,
        }
    }

    /// Returns the opposite direction.
    pub fn opposite(self) -> Self {
        match self {
            Self::North => Self::South,
            Self::East => Self::West,
            Self::South => Self::North,
            Self::West => Self::East,
            Self::Up => Self::Down,
            Self::Down => Self::Up,
        }
    }
}

impl fmt::Display for Cardinal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::North => write!(f, "north"),
            Self::East => write!(f, "east"),
            Self::South => write!(f, "south"),
            Self::West => write!(f, "west"),
            Self::Up => write!(f, "up"),
            Self::Down => write!(f, "down"),
        }
    }
}
