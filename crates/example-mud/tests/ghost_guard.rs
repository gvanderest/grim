//! Ghost-crate guard: no `path = …` dependency may point outside the workspace.
//!
//! Such a dependency compiles and ships, but `cargo clippy/test --workspace`
//! never selects the target — its tests never run, its lines never count.
//! (`members = ["crates/*"]` auto-includes in-workspace path deps, so only
//! outside ones escape. Membership itself and README presence ride on the
//! glob plus review, per the maintainer's call on #114.)
//!
//! Failure names the `from -> to (path)` edge. See issue #114 and
//! `.planning/tmp/issue-114-ghost-guard/DESIGN.md`.
//!
//! Why a custom test and not `cargo-deny`: same reason as
//! `dep_direction.rs` — `cargo-deny` cannot express intra-workspace edges.
//! This test reads `cargo metadata` directly via the `cargo_metadata` crate.

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
