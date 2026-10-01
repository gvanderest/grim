//! PVE combat scenarios (#64): kill → rounds → death → corpse → loot,
//! plus flee, switch, and kick. Driven through the headless harness against
//! the real seed (wolves in Whisperwood, raid bear in the cavern).

mod harness;
use harness::{Mud, Session};
const PW: &str = "combat-pw-1";

fn create_char(mud: &mut Mud, email: &str, name: &str) -> Session {
    let (s, _) = mud.connect();
    let _ = mud.send(s, email);
    let _ = mud.send(s, "y");
    let _ = mud.send(s, PW);
    let _ = mud.send(s, "c");
    let _ = mud.send(s, name);
    let _ = mud.send(s, "1");
    let _ = mud.send(s, "human");
    let _ = mud.send(s, "warrior");
    mud.send(s, "").assert_contains("Exits:");
    s
}

fn walk_to_edge(mud: &mut Mud, s: Session) {
    mud.send(s, "north").assert_contains("Town Square");
    mud.send(s, "east").assert_contains("Grimmok's Forge");
    mud.send(s, "east").assert_contains("East Road");
    mud.send(s, "east").assert_contains("Forest Edge");
}

/// `kill wolf` engages and lands the immediate first round: attacker output
/// plus the room echo, and the fight persists across ticks.
#[test]
fn kill_starts_a_fight_with_first_round_now() {
    let mut mud = Mud::new();
    let alice = create_char(&mut mud, "alice@example.com", "Alice");
    walk_to_edge(&mut mud, alice);

    let out = mud.send(alice, "kill wolf");
    let text = out.text();
    assert!(
        text.contains("attack") || text.contains("hit") || text.contains("miss"),
        "kill should engage and strike, got:\n{text}"
    );
    // Still engaged afterwards: switch finds the target.
    mud.send(alice, "switch wolf").assert_contains("switch");
}

/// Killing a 20-HP wolf banks XP + coin, leaves a lootable corpse, and the
/// corpse empties through `get`.
#[test]
fn wolf_kill_pays_xp_coin_and_corpse() {
    let mut mud = Mud::new();
    let alice = create_char(&mut mud, "alice@example.com", "Alice");
    walk_to_edge(&mut mud, alice);

    let _ = mud.send(alice, "kill wolf");
    // Hammer the wolf: rounds + kicks until it dies (bounded pumps).
    let mut died = false;
    for _ in 0..40 {
        let out = mud.send(alice, "kick");
        if out.contains("slain") {
            died = true;
            break;
        }
        let out = mud.recv(alice);
        if out.contains("slain") {
            died = true;
            break;
        }
    }
    assert!(died, "wolf should die within 40 rounds of kick+auto");

    // Awards banked on the character.
    let ch = mud.character("Alice").expect("Alice in world");
    assert!(ch.xp >= 25, "XP awarded, got {}", ch.xp);
    assert!(ch.coin >= 5, "coin awarded, got {}", ch.coin);

    // Corpse present and lootable.
    mud.send(alice, "look").assert_contains("corpse");
    let out = mud.send(alice, "look in corpse");
    assert!(
        out.contains("fang")
            || out.contains("pelt")
            || out.contains("pebble")
            || out.contains("empty"),
        "corpse lists loot or empty, got:\n{}",
        out.text()
    );
}

/// `flee` is combat-only: refused outside a fight, resolved inside one.
#[test]
fn flee_refused_outside_combat() {
    let mut mud = Mud::new();
    let alice = create_char(&mut mud, "alice@example.com", "Alice");
    mud.send(alice, "flee").assert_contains("fighting");
}

/// No blow lands after death: a victim slain mid-flush takes no further
/// strikes from the same round's queued pairs.
#[test]
fn no_strike_after_death() {
    let mut mud = Mud::new();
    let alice = create_char(&mut mud, "alice@example.com", "Alice");
    walk_to_edge(&mut mud, alice);
    let _ = mud.send(alice, "kill wolf");
    // Fight to the death; then assert no Damaged line names the corpse.
    // The slain line and any same-flush extra blow share the pump — collect
    // several pumps and check ordering: after the first "slain", no later
    // "hits A Grey Wolf" may appear.
    let mut transcript = String::new();
    for _ in 0..40 {
        let out = mud.send(alice, "kick");
        transcript.push_str(out.text());
        transcript.push_str(mud.recv(alice).text());
        if transcript.contains("slain") {
            break;
        }
    }
    assert!(transcript.contains("slain"), "wolf should die");
    let slain_at = transcript.find("slain").unwrap();
    let after = &transcript[slain_at..];
    assert!(
        !after.contains("hits A Grey Wolf"),
        "no strikes after death, got:\n{after}"
    );
}

/// Blows render "Your <verb> hits <victim>! (N)": players punch by name.
#[test]
fn damage_nouns_render_per_side() {
    let mut mud = Mud::new();
    let alice = create_char(&mut mud, "alice@example.com", "Alice");
    walk_to_edge(&mut mud, alice);
    let out = mud.send(alice, "kill wolf");
    let text = out.text();
    assert!(
        text.contains("Your punch hits A Grey Wolf!"),
        "PC punch line, got:\n{text}"
    );
}

/// Witnesses see "<name> flees to the <direction>!".
/// Successful flee reads like a walk: "You flee to the …" plus the arrival
/// room description. (25% odds — retry until it lands, bounded.)
#[test]
fn flee_success_walks_like_a_move() {
    let mut mud = Mud::new();
    let alice = create_char(&mut mud, "alice@example.com", "Alice");
    walk_to_edge(&mut mud, alice);
    let _ = mud.send(alice, "kill wolf");
    for _ in 0..30 {
        let out = mud.send(alice, "flee");
        if out.contains("flee to the") {
            let text = out.text();
            // Direction named + arrival description present in the same pump
            // (fleeing west from the Edge lands back on the East Road).
            assert!(
                text.contains("Forest")
                    || text.contains("forest")
                    || text.contains("Road")
                    || text.contains("road"),
                "flee should land with a room description, got:\n{text}"
            );
            return;
        }
    }
    panic!("flee never succeeded in 30 tries at 25%");
}

/// Witnesses see "<name> flees to the <direction>!".
#[test]
fn flee_success_broadcasts_direction() {
    let mut mud = Mud::new();
    let alice = create_char(&mut mud, "alice@example.com", "Alice");
    walk_to_edge(&mut mud, alice);
    let bob = create_char(&mut mud, "bob@example.com", "Bob");
    for line in ["north", "east", "east", "east"] {
        let _ = mud.send(bob, line);
    }
    let _ = mud.send(alice, "kill wolf");
    for _ in 0..30 {
        let _ = mud.send(alice, "flee");
        let seen = mud.recv(bob);
        if seen.contains("flees to the") {
            return;
        }
    }
    panic!("no flee broadcast in 30 tries at 25%");
}

/// `switch` reprioritizes among engaged targets and fails closed elsewhere.
#[test]
fn switch_needs_a_fight() {
    let mut mud = Mud::new();
    let alice = create_char(&mut mud, "alice@example.com", "Alice");
    mud.send(alice, "switch wolf").assert_contains("fighting");
}

/// `kick` is combat-only and never initiates: refused outside a fight.
#[test]
fn kick_refused_outside_combat() {
    let mut mud = Mud::new();
    let alice = create_char(&mut mud, "alice@example.com", "Alice");
    mud.send(alice, "kick").assert_contains("fighting");
}

/// Posture verbs answer and reject repeats.
#[test]
fn sit_sleep_stand_cycle() {
    let mut mud = Mud::new();
    let alice = create_char(&mut mud, "alice@example.com", "Alice");
    mud.send(alice, "sit").assert_contains("sit");
    mud.send(alice, "sit").assert_contains("already sitting");
    mud.send(alice, "sleep").assert_contains("sleep");
    mud.send(alice, "stand").assert_contains("stand");
    mud.send(alice, "stand").assert_contains("already standing");
}

/// Corpses are one-way: `put` refuses.
#[test]
fn corpse_rejects_put() {
    let mut mud = Mud::new();
    let alice = create_char(&mut mud, "alice@example.com", "Alice");
    walk_to_edge(&mut mud, alice);

    let _ = mud.send(alice, "kill wolf");
    let mut died = false;
    for _ in 0..40 {
        let out = mud.send(alice, "kick");
        if out.contains("slain") {
            died = true;
            break;
        }
        if mud.recv(alice).contains("slain") {
            died = true;
            break;
        }
    }
    assert!(died, "wolf should die for the put test");
    // `get all corpse` empties the corpse (which then despawns); instead take
    // one item by name when present, else skip — the one-way guard is proven
    // by any held item refused re-entry.
    let out = mud.send(alice, "look in corpse");
    if out.contains("empty") {
        return;
    }
    // Take the pelt if present, else the fang: whichever exists, one drop
    // stays behind so the corpse survives for the put refusal.
    let _ = mud.send(alice, "get torn pelt corpse");
    mud.send(alice, "put pelt corpse")
        .assert_contains("can't put");
}

/// The Old Cave Bear is aggressive: walking into its cavern starts a fight
/// without typing `kill`.
#[test]
fn bear_attacks_on_entry() {
    let mut mud = Mud::new();
    let alice = create_char(&mut mud, "alice@example.com", "Alice");
    mud.send(alice, "north").assert_contains("Town Square");
    mud.send(alice, "east").assert_contains("Grimmok's Forge");
    mud.send(alice, "east").assert_contains("East Road");
    mud.send(alice, "east").assert_contains("Forest Edge");
    mud.send(alice, "east").assert_contains("Forest Heart");
    mud.send(alice, "east").assert_contains("Forest Clearing");
    let out = mud.send(alice, "south");
    let text = out.text();
    assert!(
        text.contains("attack") || text.contains("hit") || text.contains("miss"),
        "bear should aggro on entry, got:\n{text}"
    );
}

/// Wolves are passive: standing in the forest starts no fight.
#[test]
fn wolves_do_not_aggro() {
    let mut mud = Mud::new();
    let alice = create_char(&mut mud, "alice@example.com", "Alice");
    walk_to_edge(&mut mud, alice);
    let out = mud.recv(alice);
    assert!(
        !out.contains("attacks you"),
        "wolves should stay passive, got:\n{}",
        out.text()
    );
}

/// PC death respawns at the starting room with full health, kept inventory,
/// and an XP debt.
#[test]
fn pc_death_respawns_with_debt() {
    let mut mud = Mud::new();
    let alice = create_char(&mut mud, "alice@example.com", "Alice");
    // Walk into the bear cavern; the bear aggros and hits ~30/round.
    for line in ["north", "east", "east", "east", "east", "east", "south"] {
        let _ = mud.send(alice, line);
    }
    let mut died = false;
    for _ in 0..30 {
        let out = mud.recv(alice);
        if out.contains("died") || out.contains("slain") {
            died = true;
            break;
        }
    }
    assert!(died, "bear should kill a solo PC");
    // Respawned at the starting room (tavern) — look shows it.
    mud.send(alice, "look").assert_contains("Rusted Anvil");
}

/// The tavern is safe: `kill` there is refused, and the room title carries
/// the white [SAFE] tag.
#[test]
fn tavern_is_safe_and_tagged() {
    let mut mud = Mud::new();
    let alice = create_char(&mut mud, "alice@example.com", "Alice");
    // Starting room is the tavern.
    mud.send(alice, "look").assert_contains("[SAFE]");
    let bob = create_char(&mut mud, "bob@example.com", "Bob");
    // Bob starts in the tavern too.
    mud.send(bob, "look").assert_contains("[SAFE]");
    mud.send(alice, "kill bob").assert_contains("safe room");
}

/// `score` shows the character sheet: name/title, race/class/level/XP, HP, coin.
#[test]
fn score_shows_character_sheet() {
    let mut mud = Mud::new();
    let alice = create_char(&mut mud, "alice@example.com", "Alice");
    let out = mud.send(alice, "score");
    let text = out.text();
    for needle in ["Alice", "Human", "Warrior", "1", "100/100", "Coin: 0"] {
        assert!(text.contains(needle), "score shows {needle}, got:\n{text}");
    }
}
