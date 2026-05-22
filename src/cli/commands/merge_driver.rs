//! `br merge-driver` — git/jj-compatible merge driver for
//! `.beads/issues.jsonl` (and any beads-shaped JSONL).
//!
//! ## Why
//!
//! `.beads/issues.jsonl` is a state-snapshot of every issue in the DB.
//! When two branches both touch beads (close one, create another,
//! comment, etc.), git's default text merger sees overlapping line
//! edits even when the changes are semantically independent ("branch A
//! closed bead X" + "branch B closed bead Y" = trivially mergeable but
//! the text merger conflicts). Every parallel feature-branch workflow
//! hits this.
//!
//! Git supports custom merge drivers per-path via `.gitattributes` +
//! `[merge "<name>"]` in git config. `br merge-driver` is the binary
//! to register there. It reuses the same `three_way_merge` logic that
//! `br sync --merge` already uses, but consumes three file paths from
//! the command line instead of looking in `.beads/`.
//!
//! ## Git merge driver protocol
//!
//! Invoked as `br merge-driver %O %A %B` where:
//!   - `%O` = ancestor (common base) file path
//!   - `%A` = "ours" file path — also where the merged result is written
//!   - `%B` = "theirs" file path
//!
//! Exit codes:
//!   - 0 = clean merge written to `%A`
//!   - 1 = unresolved conflicts; git marks the file as unmerged
//!   - other = error (propagated via the standard BeadsError exit code path)
//!
//! ## Setup in a consuming repo
//!
//! `.gitattributes`:
//! ```text
//! .beads/issues.jsonl merge=beads-jsonl
//! ```
//!
//! `.git/config` (or `git config`):
//! ```text
//! [merge "beads-jsonl"]
//!     name = beads_rust JSONL semantic merge
//!     driver = br merge-driver %O %A %B
//! ```
//!
//! For jj, the same driver can be registered via `jj`'s merge-tools
//! config — point its program at `br` and pass `merge-driver $base $left $right`.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use clap::Args;

use crate::error::{BeadsError, Result};
use crate::model::Issue;
use crate::sync::{read_issues_from_jsonl, three_way_merge, ConflictResolution, MergeContext};

/// CLI args for the `merge-driver` subcommand.
#[derive(Args, Debug, Clone)]
pub struct MergeDriverArgs {
    /// Path to the ancestor (common base) JSONL file. Git's `%O` /
    /// jj's `$base`.
    pub ancestor: PathBuf,

    /// Path to the "ours" JSONL file (current branch). Git's `%A` /
    /// jj's `$left`. Default destination for the merged result if
    /// `--output` is not specified.
    pub ours: PathBuf,

    /// Path to the "theirs" JSONL file (incoming branch). Git's `%B`
    /// / jj's `$right`.
    pub theirs: PathBuf,

    /// Explicit output path for the merged result. jj passes its
    /// separate `$output` path here. Git's protocol overwrites `%A`,
    /// so leave this unset for git. If unset, the result is written
    /// back to `OURS`.
    #[arg(long, short = 'o')]
    pub output: Option<PathBuf>,

    /// Strategy for resolving conflicts when both sides modified the
    /// same issue. Defaults to prefer-newer (uses `updated_at`).
    #[arg(long, value_enum, default_value = "prefer-newer")]
    pub strategy: ConflictResolution,

    /// Suppress all output on success. Merge drivers typically want
    /// this so commit messages and diagnostics aren't polluted.
    #[arg(long, short = 'q')]
    pub quiet: bool,
}

/// Run the merge driver. Returns the process exit code:
///   - `0` on a clean semantic merge (result written to `ours`).
///   - `1` if conflicts remain (file left as-is; git/jj will mark
///     unmerged and require human resolution).
///
/// Errors (returned as `Err`) are reserved for "couldn't even read the
/// inputs" — the dispatcher maps those to BeadsError's normal exit code.
pub fn execute(args: &MergeDriverArgs) -> Result<i32> {
    let base = load_keyed(&args.ancestor)?;
    let left = load_keyed(&args.ours)?;
    let right = load_keyed(&args.theirs)?;

    let ctx = MergeContext::new(base, left, right);
    let report = three_way_merge(&ctx, args.strategy, None);

    if !report.conflicts.is_empty() {
        if !args.quiet {
            eprintln!(
                "br merge-driver: {} unresolvable conflict(s):",
                report.conflicts.len()
            );
            for (id, kind) in &report.conflicts {
                eprintln!("  {id}: {kind:?}");
            }
            eprintln!(
                "br merge-driver: leaving {} as-is; resolve manually or re-run with --strategy",
                args.ours.display()
            );
        }
        return Ok(1);
    }

    let destination = args.output.as_deref().unwrap_or(&args.ours);
    write_jsonl(destination, &report.kept)?;

    if !args.quiet {
        eprintln!(
            "br merge-driver: merged ok ({} kept, {} deleted) → {}",
            report.kept.len(),
            report.deleted.len(),
            destination.display()
        );
    }

    Ok(0)
}

/// Read a JSONL file and key its issues by id. Empty/missing file →
/// empty map (covers the "file added on one side" case).
fn load_keyed(path: &Path) -> Result<HashMap<String, Issue>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let issues = read_issues_from_jsonl(path)?;
    Ok(issues.into_iter().map(|i| (i.id.clone(), i)).collect())
}

/// Write a set of issues to `path` as JSONL, sorted by id for
/// determinism. Atomic via temp-file-then-rename in the same dir.
fn write_jsonl(path: &Path, issues: &[Issue]) -> Result<()> {
    let mut sorted: Vec<&Issue> = issues.iter().collect();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let temp_path = {
        let mut p = path.to_path_buf();
        let mut name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "merged.jsonl".into());
        name.push_str(".br-merge-driver.tmp");
        p.set_file_name(name);
        p
    };

    {
        let file = File::create(&temp_path)?;
        let mut writer = BufWriter::new(file);
        for issue in sorted {
            serde_json::to_writer(&mut writer, issue).map_err(|e| {
                BeadsError::Config(format!("Failed to serialize issue {}: {e}", issue.id))
            })?;
            writer.write_all(b"\n")?;
        }
        writer.flush()?;
    }

    fs::rename(&temp_path, path)?;
    Ok(())
}
