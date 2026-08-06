//! End-to-end scenarios for example-mud, driven through the headless harness.
//!
//! Each test boots a fresh MUD (isolated temp data dir) and drives it as a
//! telnet user would — creating accounts, characters, moving, talking — then
//! asserts on the recorded output. Multiple related steps live in one test on
//! purpose: the flow IS the thing under test.

mod harness;
use grim::components::Gender;
use grim::Role;
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
fn legacy_character_is_routed_through_the_picker_at_login() {
    let mut mud = Mud::new();
    // Create Nomad normally, then log out so the character lives only on disk.
    let nomad = create_char(&mut mud, "nomad@example.com", "Nomad");
    let _ = mud.send(nomad, "quit");
    mud.disconnect(nomad);

    // Simulate a character created before races/classes existed: clear its
    // on-disk race/class. The account still owns it.
    mud.make_character_legacy("Nomad");

    // Log back in and open the menu.
    let (again, _) = mud.connect();
    let _ = mud.send(again, "nomad@example.com");
    mud.send(again, PW).assert_contains("Nomad");

    // Selecting the legacy character does NOT drop into the world — it opens the
    // gender picker instead.
    mud.send(again, "1").assert_contains("Choose a gender");
    mud.send(again, "male").assert_contains("Choose a race");
    mud.send(again, "human").assert_contains("Choose a class");
    // Picking the class backfills the build and enters the world (MOTD → room).
    let _ = mud.send(again, "warrior");
    mud.send(again, "").assert_contains("Exits:");

    // The build is now recorded, and level is still 1 (no XP system).
    let nomad = mud.character("Nomad").expect("Nomad is in the world");
    assert_eq!(nomad.gender, Gender::Male);
    assert_eq!(nomad.race, "human");
    assert_eq!(nomad.class, "warrior");
    assert_eq!(nomad.level, 1);
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
fn title_command_sets_clears_and_shows_in_who() {
    let mut mud = Mud::new();
    let alice = create_char(&mut mud, "alice@example.com", "Alice");

    // Setting a title confirms, and clearing it confirms too.
    mud.send(alice, "title the Bold")
        .assert_contains("the Bold");
    assert_eq!(
        mud.character("Alice").unwrap().title.as_deref(),
        Some("the Bold")
    );
    mud.send(alice, "title").assert_contains("cleared");
    assert!(mud.character("Alice").unwrap().title.is_none());

    // An over-length title (61 chars) is rejected and nothing is stored.
    let long = "z".repeat(61);
    mud.send(alice, &format!("title {long}"))
        .assert_contains("at most 60 characters");
    assert!(mud.character("Alice").unwrap().title.is_none());
}

#[test]
fn who_list_is_ordered_and_formatted_mud_style() {
    let mut mud = Mud::new();
    // Three characters enter in order: Alice, then Bob, then Carol. Creation
    // order fixes the connect-time tiebreak (Bob connected before Carol).
    let _alice = create_char(&mut mud, "alice@example.com", "Alice");
    let _bob = create_char(&mut mud, "bob@example.com", "Bob");
    let carol = create_char(&mut mud, "carol@example.com", "Carol");

    // Alice is an immortal with a title (human warrior, level irrelevant → IMM).
    mud.edit_character("Alice", |c| {
        c.roles.push(Role::Admin);
        c.title = Some("the Great".into());
    });
    // Bob: level 10 elf mage with a title.
    mud.edit_character("Bob", |c| {
        c.level = 10;
        c.race = "elf".into();
        c.class = "mage".into();
        c.title = Some("the Wise".into());
    });
    // Carol: level 10 human warrior, no title. Same level as Bob, so the
    // connect-time tiebreak (Bob first) decides their order.
    mud.edit_character("Carol", |c| c.level = 10);

    let out = mud.send(carol, "who");
    let text = out.text();

    // Exact MUD-style rows: `LLL G RRRRR CCC GGGGG Name Title`.
    let alice_row = "IMM M Human War       Alice the Great";
    let bob_row = " 10 M Elf   Mag       Bob the Wise";
    let carol_row = " 10 M Human War       Carol";
    out.assert_contains("Players online (3):")
        .assert_contains(alice_row)
        .assert_contains(bob_row)
        .assert_contains(carol_row);

    // Ordering: admin first (alpha), then level-10 by connect time (Bob<Carol).
    let ai = text.find(alice_row).unwrap();
    let bi = text.find(bob_row).unwrap();
    let ci = text.find(carol_row).unwrap();
    assert!(ai < bi && bi < ci, "WHO order wrong:\n{text}");
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
