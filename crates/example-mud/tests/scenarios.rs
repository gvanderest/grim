//! End-to-end scenarios for example-mud, driven through the headless harness.
//!
//! Each test boots a fresh MUD (isolated temp data dir) and drives it as a
//! telnet user would — creating accounts, characters, moving, talking — then
//! asserts on the recorded output. Multiple related steps live in one test on
//! purpose: the flow IS the thing under test.

mod harness;
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
    let _ = mud.send(s, name); //  name it → MOTD
    let _ = mud.send(s, ""); //    press enter at MOTD → enter the world
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
fn quit_then_reconnect_resumes_the_same_character() {
    let mut mud = Mud::new();
    let alice = create_char(&mut mud, "alice@example.com", "Alice");
    mud.send(alice, "quit").assert_contains("disconnected");
    mud.disconnect(alice);

    // Character persists in the world (linkdead), not destroyed.
    assert!(mud.character_names().contains(&"Alice".to_string()));

    // Reconnect on a fresh connection: existing account → password → pick char.
    let (again, _) = mud.connect();
    mud.send(again, "alice@example.com")
        .assert_contains("Password");
    mud.send(again, PW)
        .assert_contains("Alice")
        .assert_contains("linkdead");
    // Selecting the character re-enters the world (linkdead reconnect skips MOTD).
    mud.send(again, "1").assert_contains("The Rusted Anvil");
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
    // Too-short password is rejected; no account/character is created.
    mud.send(s, "pw").assert_excludes("Characters");
    assert!(!mud.character_names().contains(&"Alice".to_string()));
}
