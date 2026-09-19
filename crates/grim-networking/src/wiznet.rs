//! Admin alert surface: the [`WiznetAlert`] message, its [`WiznetCategory`],
//! and the [`admin_log!`] macro for emitting one-liners from anywhere with
//! a `MessageWriter<WiznetAlert>` in scope.
//!
//! Producers (transports, the auth gate, session systems) categorize
//! operational events; `grim-scene` broadcasts them to online admins whose
//! wiznet prefs (see `grim-config`, keys `wiznet.*`) opt into the category.
//! Alert bodies are operational log lines (addresses, names, reasons), not
//! author-facing prose — they stay out of the text catalog by design.

use bevy::prelude::*;

/// Admin-alert category. Fixed set of two; a new category is a new variant
/// plus producers — the `wiznet` command itself is key-driven off the
/// `wiznet.*` config keys and needs no change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WiznetCategory {
    /// Abuse-relevant: guard trips, shed transitions, throttle and ban
    /// refusals.
    Security,
    /// Socket churn and world entry: connects, closes, linkdead,
    /// reconnects, quits, logins.
    Logins,
}

impl std::fmt::Display for WiznetCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Security => write!(f, "security"),
            Self::Logins => write!(f, "logins"),
        }
    }
}

/// One admin-alert line. Broadcast to opted-in online admins by
/// `grim-scene`; never shown to players.
#[derive(Message, Debug, Clone)]
pub struct WiznetAlert {
    pub category: WiznetCategory,
    pub text: String,
}

/// Emit an admin alert from a system holding the writer:
/// `admin_log!(alerts, WiznetCategory::Security, "conn {} tripped", id)`.
/// Thin format-forwarding over [`WiznetAlert`]; the writer stays an
/// explicit system param because Bevy must thread it — a macro cannot
/// invent params.
#[macro_export]
macro_rules! admin_log {
    ($writer:expr, $category:expr, $($arg:tt)*) => {
        $writer.write($crate::WiznetAlert {
            category: $category,
            text: format!($($arg)*),
        })
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_display_as_config_keys() {
        assert_eq!(WiznetCategory::Security.to_string(), "security");
        assert_eq!(WiznetCategory::Logins.to_string(), "logins");
    }

    #[test]
    fn admin_log_writes_categorized_alert() {
        let mut app = App::new();
        app.add_message::<WiznetAlert>();
        app.add_systems(Update, |mut alerts: MessageWriter<WiznetAlert>| {
            admin_log!(alerts, WiznetCategory::Security, "conn {} tripped", 7);
        });
        app.update();
        let msgs = app.world().resource::<Messages<WiznetAlert>>();
        let mut cursor = msgs.get_cursor();
        let got: Vec<&WiznetAlert> = cursor.read(msgs).collect();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].category, WiznetCategory::Security);
        assert_eq!(got[0].text, "conn 7 tripped");
    }
}
