//! Source identity of a checkout: its commit and whether it is verifiably clean.
//!
//! Shared by `record_snapshot` tests and `build.rs`, which embeds the result
//! so a capture names the source its binary was compiled from.

use std::path::Path;

/// The commit of `repo_root` with a `-dirty` suffix unless the tree is
/// verifiably clean, or `None` when `repo_root` is not itself a repository root.
///
/// A directory merely inside another repository has no identity of its own.
pub(crate) fn git_sha(repo_root: &Path) -> Option<String> {
    if git_output(repo_root, &["rev-parse", "--show-prefix"])?.trim() != "" {
        return None;
    }
    let sha = git_output(repo_root, &["rev-parse", "HEAD"])?;
    let sha = sha.trim();
    if sha.is_empty() {
        return None;
    }
    let worktree_dirty_or_unknown = git_output(repo_root, &["status", "--porcelain"])
        .is_none_or(|status| !status.trim().is_empty())
        || has_hidden_index_entries(repo_root);
    if worktree_dirty_or_unknown {
        Some(format!("{sha}-dirty"))
    } else {
        Some(sha.to_owned())
    }
}

/// Whether the index hides tracked files from `git status`, or cannot be read.
///
/// Assume-unchanged (lowercase tag in `git ls-files -v`) and skip-worktree
/// (`S`) entries make `git status --porcelain` blind to edits of tracked files
/// that Cargo still compiles, so an empty status cannot show such a tree is
/// clean. The ROM-build checkout verifier rejects the same entries.
fn has_hidden_index_entries(repo_root: &Path) -> bool {
    git_output(repo_root, &["ls-files", "-v"]).is_none_or(|listing| {
        listing
            .lines()
            .any(|line| line.starts_with(|c: char| c.is_ascii_lowercase() || c == 'S'))
    })
}

/// Git variables that redirect which repository, worktree, index, object
/// store, or configuration a command reads. Inherited from a caller such as a
/// git hook, they would attribute a capture to another repository.
const GIT_ROUTING_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_COMMON_DIR",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_PREFIX",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_CEILING_DIRECTORIES",
    "GIT_GRAFT_FILE",
    "GIT_SHALLOW_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
];

/// A `git -C <repo_root>` command isolated from inherited repository routing.
pub(crate) fn git_command(repo_root: &Path) -> std::process::Command {
    let mut command = std::process::Command::new("git");
    // A read-only probe must not rewrite the index under a concurrent git.
    command.arg("--no-optional-locks").arg("-C").arg(repo_root);
    for name in GIT_ROUTING_ENV {
        command.env_remove(name);
    }
    // `GIT_CONFIG_KEY_<n>` / `GIT_CONFIG_VALUE_<n>` are numbered.
    for (name, _) in std::env::vars_os() {
        let numbered_config = name.to_str().is_some_and(|name| {
            name.starts_with("GIT_CONFIG_KEY_") || name.starts_with("GIT_CONFIG_VALUE_")
        });
        if numbered_config {
            command.env_remove(name);
        }
    }
    command
}

pub(crate) fn git_output(repo_root: &Path, args: &[&str]) -> Option<String> {
    let output = git_command(repo_root).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

