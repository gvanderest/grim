//! Crate dependency-direction / coupling guard.
//!
//! Encodes the internal crate dependency graph that `docs/ARCHITECTURE.md` §3–§4
//! describe (the crate map + "Dependency direction" diagram) as an allow-list and
//! asserts **set-equality** against the graph `cargo metadata` reports today.
//!
//! The test fails on BOTH:
//!   * an **unexpected** edge — a new intra-workspace dependency nobody sanctioned;
//!   * a **stale** allow-list entry — an edge listed here that no longer exists.
//!
//! Either way the failure names the offending edge(s), so a coupling regression is
//! caught at `cargo test` rather than in review.
//!
//! Why a custom test and not `cargo-deny`: `cargo-deny`'s `bans` operate on crate
//! identities/features, not on "crate A may depend on crate B" intra-workspace
//! edges. Expressing a directed allow-list of workspace edges — and flagging stale
//! entries — is exactly what it cannot do cleanly, so we read `cargo metadata`
//! directly via the `cargo_metadata` crate.
//!
//! ── §4 vs. reality ──────────────────────────────────────────────────────────
//! The ALLOWED list below is the source of truth for what the graph *is*. Where it
//! diverges from §4's table/diagram, the edge is still encoded (reality wins) with a
//! `// NOTE:` flagging the divergence for review. The headline divergences:
//!
//!  1. `grim-core` — the transitional "god-types" node (ARCHITECTURE.md §8).
//!     §4's table does not list it, yet most subsystems depend on it. Encoded here.
//!  2. `grim-scene`'s DEV-only harness edges to `grim-world` / `grim-channel`
//!     (`[dev-dependencies]`, ARCHITECTURE.md §10). Carved out as `dev` edges so a
//!     dev-only coupling can never masquerade as a normal one.
//!  3. §4's diagram routes `grim-world` / `grim-channel` through `grim-command`.
//!     Reality: they do not depend on `grim-command` at all (command dispatch is
//!     mediated by `grim-scene`); they depend on `grim-core` etc. instead.
//!  4. The facade `grim` and the `example-mud` binary are absent from §4's
//!     subsystem-direction diagram; both are encoded here.

use std::collections::BTreeSet;

use cargo_metadata::{DependencyKind, MetadataCommand};

/// A directed internal edge `from -> to`, both workspace crates.
type Edge = (&'static str, &'static str);

/// Internal edges expected with dependency kind **normal** (`[dependencies]`).
///
/// Grouped by the depending crate and ordered to mirror ARCHITECTURE.md §4 — the
/// foundation crates first, subsystems next, facade + binary last.
const ALLOWED_NORMAL: &[Edge] = &[
    // ── Foundation ──────────────────────────────────────────────────────────
    // `grim-color` depends on nothing internal (Bevy-free, serde-free leaf). §4.
    // `grim-command` "depends on nothing but Bevy" — confirmed, no internal edges. §4.
    //
    // `grim-text` → `grim-color`: the catalog converts/escapes colour markup. §4.
    ("grim-text", "grim-color"),
    // NOTE (divergence #1): `grim-core` is the transitional god-types node
    // (§8). It is not in §4's crate-map table, but it exists and re-exports colour.
    ("grim-core", "grim-color"),
    // ── Transports ──────────────────────────────────────────────────────────
    // `grim-networking` depends on nothing internal (Bevy + serde only).
    // `grim-networking-telnet` renders (→ grim-color) and rides the bridge
    // (→ grim-networking); →grim-core is the god-types coupling (#1). §4/§8.
    ("grim-networking-telnet", "grim-color"),
    ("grim-networking-telnet", "grim-networking"),
    ("grim-networking-telnet", "grim-core"),
    // NOTE (Placement Phase 2a step 2): copyover re-adopts characters, reading the
    // Character/Linkdead beings, so telnet depends on grim-actor. No cycle.
    ("grim-networking-telnet", "grim-actor"),
    // ── Session / scene ───────────────────────────────────────────────────────
    // `grim-scene` is the bridge between networking and commands (§5.3): it reads
    // ConnectionInput and dispatches Commands. Hence → grim-networking, grim-command.
    ("grim-scene", "grim-command"),
    ("grim-scene", "grim-networking"),
    ("grim-scene", "grim-color"),
    ("grim-scene", "grim-text"),
    ("grim-scene", "grim-core"), // god-types coupling (#1)
    // NOTE (divergence): §4 does not show scene→persistence; today the scene owns
    // resume placement / player-alias expansion, so it depends on grim-persistence.
    ("grim-scene", "grim-persistence"),
    // NOTE (Placement Phase 1): the race/class registry content moved into
    // grim-world, and the scene's creation flow reads it in production code
    // (params.rs / plugin.rs), so scene→world is now a NORMAL edge (previously a
    // dev-only harness edge). grim-world does not depend on grim-scene, so this
    // adds no cycle.
    ("grim-scene", "grim-world"),
    // NOTE (Placement Phase 2a step 2): the session reads beings (Character/
    // Player/InRoom/Linkdead/OutputHistory) throughout login/creation/resume/
    // output, so scene → grim-actor is a normal edge. grim-actor does not depend
    // on grim-scene, so no cycle.
    ("grim-scene", "grim-actor"),
    // ── Pre-game / auth ─────────────────────────────────────────────────────────
    // NOTE (Phase 2b): grim-auth owns the login / account-creation /
    // character-select / MOTD flow extracted from grim-scene. It is the pre-game
    // phase LAYERED ON the session core, so grim-auth → grim-scene is a normal
    // edge (it reuses scene's shared formatter + ConnectedAt/JustEnteredWorld/
    // SceneSystems). grim-scene does NOT depend on grim-auth — verified by grep
    // and by the absence of the reverse edge here. The remaining edges mirror the
    // flow's reads: beings (grim-actor), room/registry topology (grim-world),
    // account/character disk I/O (grim-persistence), wire types (grim-networking),
    // catalog (grim-text), banner rendering (grim-color), and the god-types node
    // (grim-core). No grim-command edge — the pre-game phase parses no commands.
    ("grim-auth", "grim-core"),
    ("grim-auth", "grim-scene"),
    ("grim-auth", "grim-actor"),
    ("grim-auth", "grim-world"),
    ("grim-auth", "grim-persistence"),
    ("grim-auth", "grim-networking"),
    ("grim-auth", "grim-text"),
    ("grim-auth", "grim-color"),
    // ── Gameplay subsystems ────────────────────────────────────────────────────
    // NOTE (divergence #3): §4's diagram routes grim-world / grim-channel through
    // grim-command. Reality: neither depends on grim-command; dispatch is mediated
    // by grim-scene. grim-world is being-free: it owns topology + shutdown signal
    // machinery only. NOTE (Placement Phase 2a step 2): grim-world → grim-networking
    // and grim-world → grim-color were REMOVED — their only users (the `quit`
    // handler's DisconnectRequest, and the `goto`/`title` escape_codes calls) moved
    // into grim-actor, so those edges are now stale.
    ("grim-world", "grim-core"),
    // NOTE (Placement Phase 2a step 2): grim-actor owns the "beings"
    // (Character/Player/InRoom/Linkdead/OutputHistory/Role) and the being-reading
    // verbs (look/move/goto/quit/title + the admin shutdown gate). It depends on
    // grim-world (topology + shutdown machinery), grim-networking (DisconnectRequest
    // for `quit`), grim-color (escape_codes for `goto`/`title`), and the god-types
    // node. grim-world/grim-core do NOT depend on grim-actor — the actor
    // layer sits strictly above the being-free world.
    ("grim-actor", "grim-core"),
    ("grim-actor", "grim-world"),
    ("grim-actor", "grim-networking"),
    ("grim-actor", "grim-color"),
    ("grim-channel", "grim-core"),
    ("grim-channel", "grim-text"),
    // NOTE (Placement Phase 2a step 2): grim-channel reads beings (Character/
    // Player/InRoom), so it depends on grim-actor. grim-actor does not depend on
    // grim-channel, so this adds no cycle.
    ("grim-channel", "grim-actor"),
    // NOTE (Placement Phase 2a): the world-topology types (Area/Room/Exits/
    // StartingRoom) moved into grim-world. grim-channel reads `Room`, so it now
    // depends on grim-world. grim-world does not depend on grim-channel, so this
    // adds no cycle.
    ("grim-channel", "grim-world"),
    // `grim-persistence` loads/saves accounts + characters (→ god-types) and reacts
    // to connection lifecycle events (→ grim-networking).
    ("grim-persistence", "grim-core"),
    ("grim-persistence", "grim-networking"),
    // NOTE (Placement Phase 2a step 2): persistence loads/saves beings
    // (Character/Player/InRoom/Linkdead/OutputHistory), so it depends on
    // grim-actor. grim-actor does not depend on grim-persistence, so no cycle.
    ("grim-persistence", "grim-actor"),
    // NOTE (Placement Phase 2a): persistence reads `Area`/`Room` (world topology
    // moved into grim-world), so it now depends on grim-world. grim-world does not
    // depend on grim-persistence, so this adds no cycle.
    ("grim-persistence", "grim-world"),
    // ── Facade + binary ────────────────────────────────────────────────────────
    // NOTE (divergence #4): absent from §4's subsystem diagram. The facade `grim`
    // depends on and re-exports every subsystem (GrimDefaultPlugins, §1/§8 step 9);
    // nothing depends back on it. `example-mud` depends on the facade alone.
    ("grim", "grim-core"),
    ("grim", "grim-text"),
    ("grim", "grim-command"),
    ("grim", "grim-networking"),
    ("grim", "grim-networking-telnet"),
    ("grim", "grim-scene"),
    // NOTE (Phase 2b): the facade re-exports grim-auth (AuthPlugin +
    // ReservedNamePrefixes) and adds AuthPlugin to the default plugin groups.
    ("grim", "grim-auth"),
    ("grim", "grim-world"),
    // NOTE (Placement Phase 2a step 2): the facade re-exports the actor beings +
    // ActorPlugin and adds ActorPlugin to GrimDefaultPlugins.
    ("grim", "grim-actor"),
    ("grim", "grim-channel"),
    ("grim", "grim-persistence"),
    ("example-mud", "grim"),
];

/// Internal edges expected with dependency kind **dev** (`[dev-dependencies]`).
///
/// NOTE (divergence #2): `grim-scene`'s E2E-style harness composes the gameplay
/// plugins to exercise the full loop (ARCHITECTURE.md §10). This is DEV-only and
/// deliberately not a normal edge — a normal dependency here would be a real coupling
/// regression, so it is segregated into its own allow-list. (The scene→world edge was
/// promoted to a normal dependency in Placement Phase 1; see ALLOWED_NORMAL.)
const ALLOWED_DEV: &[Edge] = &[
    ("grim-scene", "grim-channel"),
    // NOTE (Phase 2b): grim-auth's E2E-style harness composes the channel plugin
    // to exercise the full login → in-game loop (mirrors grim-scene's harness).
    // DEV-only; a normal edge here would be a coupling regression.
    ("grim-auth", "grim-channel"),
];

/// Read the workspace graph and return internal edges split by kind: `(normal, dev)`.
/// Only edges where both endpoints are workspace members are considered.
fn actual_edges() -> (BTreeSet<Edge>, BTreeSet<Edge>) {
    // Leak the metadata so the `&'static str` edge type is satisfiable without
    // cloning every crate name into owned strings. The process is a short-lived
    // test binary, so the one-time leak is inconsequential.
    let metadata: &'static cargo_metadata::Metadata = Box::leak(Box::new(
        MetadataCommand::new()
            // Pin the manifest so the guard doesn't depend on the runner's cwd
            // (a runner starting the binary elsewhere would read a different
            // manifest or fail).
            .manifest_path(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
            .no_deps()
            .exec()
            .expect("cargo metadata failed"),
    ));

    // Workspace-member crate names — the only endpoints we treat as "internal".
    let members: BTreeSet<&str> = metadata
        .workspace_members
        .iter()
        .filter_map(|id| {
            metadata
                .packages
                .iter()
                .find(|p| &p.id == id)
                .map(|p| p.name.as_str())
        })
        .collect();

    let mut normal = BTreeSet::new();
    let mut dev = BTreeSet::new();
    // Build/other-kind edges are a separate coupling axis with no allow-list.
    // Collect them apart from `normal` — folding them in would let a build edge
    // that shares endpoints with an existing normal edge be silently absorbed by
    // set dedup, hiding the new build-time coupling.
    let mut other: BTreeSet<(&str, &str, String)> = BTreeSet::new();

    for pkg in &metadata.packages {
        let from = pkg.name.as_str();
        if !members.contains(from) {
            continue;
        }
        for dep in &pkg.dependencies {
            let to = dep.name.as_str();
            if !members.contains(to) {
                continue; // external crate (bevy, serde, …) — not our concern.
            }
            match dep.kind {
                DependencyKind::Normal => {
                    normal.insert((from, to));
                }
                DependencyKind::Development => {
                    dev.insert((from, to));
                }
                kind => {
                    other.insert((from, to, format!("{kind:?}")));
                }
            }
        }
    }

    // No internal build (or other-kind) edges are expected today. Surface any
    // regardless of whether the same pair exists as a normal/dev edge.
    assert!(
        other.is_empty(),
        "unexpected internal non-normal/dev edge(s): {:?}",
        other
    );

    (normal, dev)
}

/// Render a set of edges as sorted `from -> to` lines for a failure message.
fn render(edges: &BTreeSet<Edge>) -> String {
    edges
        .iter()
        .map(|(f, t)| format!("    {f} -> {t}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn dependency_direction_matches_architecture() {
    let (actual_normal, actual_dev) = actual_edges();
    let allowed_normal: BTreeSet<Edge> = ALLOWED_NORMAL.iter().copied().collect();
    let allowed_dev: BTreeSet<Edge> = ALLOWED_DEV.iter().copied().collect();

    // Sanity: the literal lists must not contain accidental duplicates.
    assert_eq!(
        allowed_normal.len(),
        ALLOWED_NORMAL.len(),
        "duplicate entry in ALLOWED_NORMAL"
    );
    assert_eq!(
        allowed_dev.len(),
        ALLOWED_DEV.len(),
        "duplicate entry in ALLOWED_DEV"
    );

    // Unexpected: present in the graph, absent from the allow-list.
    let unexpected_normal: BTreeSet<Edge> =
        actual_normal.difference(&allowed_normal).copied().collect();
    let unexpected_dev: BTreeSet<Edge> = actual_dev.difference(&allowed_dev).copied().collect();
    // Stale: listed in the allow-list, no longer in the graph.
    let stale_normal: BTreeSet<Edge> = allowed_normal.difference(&actual_normal).copied().collect();
    let stale_dev: BTreeSet<Edge> = allowed_dev.difference(&actual_dev).copied().collect();

    // Build the message unconditionally (it is empty on the happy path) so this code
    // executes every run and the assertion carries a fully-formed diagnostic.
    let mut msg = String::from(
        "internal crate dependency graph does not match the ALLOWED list \
         (docs/ARCHITECTURE.md §4).\n",
    );
    if !unexpected_normal.is_empty() {
        msg.push_str(&format!(
            "\nUNEXPECTED normal edge(s) — add to ALLOWED_NORMAL only if intended:\n{}\n",
            render(&unexpected_normal)
        ));
    }
    if !unexpected_dev.is_empty() {
        msg.push_str(&format!(
            "\nUNEXPECTED dev edge(s) — add to ALLOWED_DEV only if intended:\n{}\n",
            render(&unexpected_dev)
        ));
    }
    if !stale_normal.is_empty() {
        msg.push_str(&format!(
            "\nSTALE normal edge(s) — edge gone, remove from ALLOWED_NORMAL:\n{}\n",
            render(&stale_normal)
        ));
    }
    if !stale_dev.is_empty() {
        msg.push_str(&format!(
            "\nSTALE dev edge(s) — edge gone, remove from ALLOWED_DEV:\n{}\n",
            render(&stale_dev)
        ));
    }

    let ok = unexpected_normal.is_empty()
        && unexpected_dev.is_empty()
        && stale_normal.is_empty()
        && stale_dev.is_empty();
    assert!(ok, "{msg}");
}
