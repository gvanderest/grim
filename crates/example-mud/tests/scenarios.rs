//! End-to-end scenarios for example-mud, driven through the headless harness.
//!
//! Each test boots a fresh MUD (isolated temp data dir) and drives it as a
//! telnet user would — creating accounts, characters, moving, talking — then
//! asserts on the recorded output. Multiple related steps live in one test on
//! purpose: the flow IS the thing under test.

mod harness;
use grim::components::Gender;
use harness::{Mud, Session};

/// A password that satisfies validation (short ones are rejected — see
/// `password_must_be_valid`).
const PW: &str = "secretpw";

/// Create a brand-new account + character and enter the world. Leaves the
/// session in-game, standing in the starting room.
fn create_char(mud: &mut Mud, email: &str, name: &str) -> Session {
    let (s, _) = mud.connect();
    let _ = mud.send(s, email); // unknown email → offered account creation
    let _ = mud.send(s, "y"); //   confirm create
    let _ = mud.send(s, PW); //    choose password → account created, character menu
    let _ = mud.send(s, "c"); //   create a character
    let _ = mud.send(s, name); //  name it → gender picker
    let _ = mud.send(s, "1"); //   gender: Male (menu index)
    let _ = mud.send(s, "human"); // race: by slug
    let _ = mud.send(s, "warrior"); // class: by slug → MOTD
                                    // Press enter at the MOTD → enter the world. Assert we actually landed in
                                    // a room, so a broken login/creation step can't return a bogus Session.
    mud.send(s, "").assert_contains("Exits:");
    s
}

#[test]
fn connect_shows_login_banner_and_prompt() {
    let mut mud = Mud::new();
    let (_s, banner) = mud.connect();
    banner.assert_contains("character name or email");
}

#[test]
fn account_creation_places_character_in_the_world() {
    let mut mud = Mud::new();
    assert!(!mud.character_names().contains(&"Alice".to_string()));

    let alice = create_char(&mut mud, "alice@example.com", "Alice");

    // The character now exists and is standing in the seeded starting room.
    assert!(mud.character_names().contains(&"Alice".to_string()));
    mud.send(alice, "look")
        .assert_contains("The Rusted Anvil")
        .assert_contains("Exits: north");
}

#[test]
fn character_creation_records_gender_race_and_class_at_level_one() {
    let mut mud = Mud::new();
    let (s, _) = mud.connect();
    let _ = mud.send(s, "gwen@example.com");
    let _ = mud.send(s, "y");
    let _ = mud.send(s, PW);
    let _ = mud.send(s, "c");
    let _ = mud.send(s, "Gwen"); // → gender picker

    // Mix input styles: gender by name prefix, race by index, class by slug.
    mud.send(s, "fem").assert_contains("Choose a race"); // Female by prefix
    mud.send(s, "2").assert_contains("Choose a class"); // Race index 2 → Elf
    let _ = mud.send(s, "mage"); // Class by slug → MOTD
    mud.send(s, "").assert_contains("Exits:"); // enter the world

    let gwen = mud.character("Gwen").expect("Gwen is in the world");
    assert_eq!(gwen.gender, Gender::Female);
    assert_eq!(gwen.race, "elf");
    assert_eq!(gwen.class, "mage");
    assert_eq!(gwen.level, 1);
}

#[test]
fn invalid_creation_pick_reprompts_without_advancing() {
    let mut mud = Mud::new();
    let (s, _) = mud.connect();
    let _ = mud.send(s, "ivy@example.com");
    let _ = mud.send(s, "y");
    let _ = mud.send(s, PW);
    let _ = mud.send(s, "c");
    let _ = mud.send(s, "Ivy"); // → gender picker

    // Out-of-range index is rejected and the gender menu is shown again.
    mud.send(s, "9")
        .assert_contains("Please choose one of the options")
        .assert_contains("Choose a gender");
    // A valid pick then advances.
    mud.send(s, "1").assert_contains("Choose a race");
    // A tier-2 class is NOT offered/creatable: its slug does not resolve.
    let _ = mud.send(s, "human");
    mud.send(s, "champion")
        .assert_contains("Please choose one of the options")
        .assert_contains("Choose a class");
    let _ = mud.send(s, "warrior"); // valid tier-1 → MOTD
    mud.send(s, "").assert_contains("Exits:");

    let ivy = mud.character("Ivy").expect("Ivy is in the world");
    assert_eq!(ivy.class, "warrior");
}

#[test]
fn movement_walks_between_seeded_rooms() {
    let mut mud = Mud::new();
    let alice = create_char(&mut mud, "alice@example.com", "Alice");

    mud.send(alice, "north")
        .assert_contains("Town Square")
        .assert_contains("Exits: east, south");
    mud.send(alice, "south").assert_contains("The Rusted Anvil");
}

#[test]
fn speech_is_heard_by_others_in_the_room() {
    let mut mud = Mud::new();
    let alice = create_char(&mut mud, "alice@example.com", "Alice");
    let bob = create_char(&mut mud, "bob@example.com", "Bob");

    // Actor sees the first-party echo.
    mud.send(alice, "say hello there")
        .assert_contains("You say")
        .assert_contains("hello there");

    // Bob, in the same room, receives it passively as third-party speech.
    mud.recv(bob)
        .assert_contains("Alice")
        .assert_contains("hello there");
}

#[test]
fn ooc_is_global_and_reaches_a_distant_player() {
    let mut mud = Mud::new();
    let alice = create_char(&mut mud, "alice@example.com", "Alice");
    let bob = create_char(&mut mud, "bob@example.com", "Bob");

    // Bob walks away to another room.
    mud.send(bob, "north").assert_contains("Town Square");

    // OOC is global, so Alice's message still reaches Bob.
    mud.send(alice, "ooc anyone around?")
        .assert_contains("anyone around?");
    mud.recv(bob).assert_contains("anyone around?");
}

#[test]
fn quit_saves_and_unloads_then_reconnect_logs_in_fresh() {
    let mut mud = Mud::new();
    let alice = create_char(&mut mud, "alice@example.com", "Alice");

    // `quit` is an intentional logout: it saves and UNLOADS the character from
    // the world. It must NOT go linkdead (that is only for an unexpected socket
    // drop). The character lives only on disk now — no in-world entity remains.
    let _ = mud.send(alice, "quit");
    mud.disconnect(alice);
    assert!(
        !mud.character_names().contains(&"Alice".to_string()),
        "quit must unload the character from the world (disk-only when logged out)"
    );

    // Reconnect is a normal login, not a linkdead reconnect: existing account →
    // password → character menu (Alice listed, not "linkdead") → select → MOTD →
    // enter the world.
    let (again, _) = mud.connect();
    mud.send(again, "alice@example.com")
        .assert_contains("Password");
    mud.send(again, PW)
        .assert_contains("Alice")
        .assert_excludes("linkdead");
    let _ = mud.send(again, "1"); // select from the menu → MOTD
    mud.send(again, "").assert_contains("The Rusted Anvil");
}

#[test]
fn a_new_account_cannot_see_another_accounts_characters() {
    let mut mud = Mud::new();
    // Account A owns Alice.
    let _alice = create_char(&mut mud, "alice@example.com", "Alice");

    // Account B is created fresh; its character menu must not leak Alice.
    let (b, _) = mud.connect();
    let _ = mud.send(b, "bob@example.com");
    let _ = mud.send(b, "y");
    mud.send(b, PW)
        .assert_contains("no characters")
        .assert_excludes("Alice");
}

#[test]
fn password_must_be_valid() {
    let mut mud = Mud::new();
    let (s, _) = mud.connect();
    let _ = mud.send(s, "alice@example.com");
    let _ = mud.send(s, "y");
    // Too-short password is rejected with a validation error, not accepted;
    // the flow stays on the password prompt and no account is created.
    mud.send(s, "pw")
        .assert_contains("at least 6 characters")
        .assert_excludes("Characters");
    assert!(mud.character_names().is_empty());

    // And nothing was persisted: a fresh connection with that email is still
    // offered account creation, not an existing-account password prompt.
    let (s2, _) = mud.connect();
    mud.send(s2, "alice@example.com")
        .assert_contains("create an account")
        .assert_excludes("Password");
}
