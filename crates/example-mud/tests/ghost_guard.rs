//! Ghost-crate guards.
//!
//! Two traps `cargo` will not catch for us:
//!
//! * A `path = …` dependency pointing **outside** the workspace compiles and
//!   ships, but `cargo clippy/test --workspace` never selects the target —
//!   its tests never run, its lines never count. (`members = ["crates/*"]`
//!   auto-includes in-workspace path deps, so only outside ones escape.)
//! * A `crates/*` dir without a `README.md` breaches the per-crate map rule
//!   (AGENTS.md 7a) silently.
//!
//! Both fail here, naming the crate or edge. See issue #114 and
//! `.planning/tmp/issue-114-ghost-guard/DESIGN.md`.
//!
//! Why a custom test and not `cargo-deny`: same reason as
//! `dep_direction.rs` — `cargo-deny` cannot express intra-workspace edges.
//! Both tests read `cargo metadata` directly via the `cargo_metadata` crate.

use std::collections::{BTreeMap, BTreeSet};

/// Workspace metadata, leaked so `&str` borrows live for the test binary's
/// life. Same trick as `dep_direction.rs`: the process is a short-lived test
/// binary, so the one-time leak is inconsequential.
fn metadata() -> &'static cargo_metadata::Metadata {
    Box::leak(Box::new(
        cargo_metadata::MetadataCommand::new()
            // Pin the manifest so the guard doesn't depend on the runner's cwd
            // (a runner starting the binary elsewhere would read a different
            // manifest or fail).
            .manifest_path(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
            .no_deps()
            .exec()
            .expect("cargo metadata failed"),
    ))
}

/// Every `path = …` dependency of a member must resolve to a workspace member.
/// Failure names the `from -> to (path)` edge.
#[test]
fn every_path_dep_resolves_inside_the_workspace() {
    let metadata = metadata();
    let member_ids: BTreeSet<&cargo_metadata::PackageId> =
        metadata.workspace_members.iter().collect();
    let by_name: BTreeMap<&str, &cargo_metadata::Package> = metadata
        .packages
        .iter()
        .map(|pkg| (pkg.name.as_str(), pkg))
        .collect();

    let mut outside = Vec::new();
    for pkg in &metadata.packages {
        if !member_ids.contains(&pkg.id) {
            continue; // external crate (bevy, serde, …) — not our concern.
        }
        for dep in &pkg.dependencies {
            if let Some(path) = dep.path.as_ref() {
                let effective = dep.rename.as_deref().unwrap_or(dep.name.as_str());
                let inside = by_name
                    .get(effective)
                    .is_some_and(|target| member_ids.contains(&target.id));
                if !inside {
                    outside.push(format!("{} -> {effective} ({path})", pkg.name));
                }
            }
        }
    }

    assert!(
        outside.is_empty(),
        "path dependencies resolving outside the workspace:\n{}",
        outside.join("\n")
    );
}

/// Every `crates/*` dir holding a `Cargo.toml` must hold a `README.md`.
/// Failure names the dir.
#[test]
fn every_crate_dir_has_a_readme() {
    let metadata = metadata();
    let crates_dir = metadata.workspace_root.join("crates");
    let entries =
        std::fs::read_dir(crates_dir.as_std_path()).expect("workspace crates dir unreadable");

    let mut missing = Vec::new();
    for entry in entries {
        let path = entry.expect("crates dir entry unreadable").path();
        if !path.is_dir() || !path.join("Cargo.toml").is_file() {
            continue;
        }
        if !path.join("README.md").is_file() {
            missing.push(
                path.file_name()
                    .expect("dir has a name")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    missing.sort();

    assert!(
        missing.is_empty(),
        "crates without README.md:\n{}",
        missing.join("\n")
    );
}
