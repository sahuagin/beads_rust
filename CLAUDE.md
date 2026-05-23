# beads_rust (br) — Agent Context

**Status**: FreeBSD-ready to build with standard Rust toolchain. No platform-specific friction detected for 15.0-CURRENT.

---

## Purpose & Philosophy

**beads_rust** is a local-first, dependency-aware issue tracker frozen at Steve Yegge's "classic" architecture (SQLite + JSONL). It enables AI agents and developers to track issues **offline, in-repo, without context switching**—replacing GitHub Issues, Linear, Jira, or scattered TODO comments.

Unlike the original beads (Go), which auto-commits, runs daemons, and installs hooks, br is **radically non-invasive**: it lives in `.beads/`, exports to JSONL for git, and never touches your git workflow without explicit commands.

**Core thesis**: A well-designed local issue tracker eliminates friction between agents, developers, and version control. No network, no accounts, no SaaS tax.

---

## Architecture

### Storage Layer

**Primary**: SQLite (fsqlite 0.1.2+, pure-Rust, concurrent-writer safe)
- Database: `.beads/beads.db` (5-8 MB binary vs 30+ MB Go binary)
- WAL mode enabled for concurrency
- Write locks via advisory filesystem locks (portable to FreeBSD)
- Dirty-flag tracking for smart JSONL exports

**Versioning Export**: JSONL (line-delimited JSON)
- File: `.beads/issues.jsonl` (git-friendly, one issue per line)
- Atomic writes to temp file, then rename
- No automatic commits—explicit `br sync --flush-only` required
- Merges cleanly in git (line-based conflicts are rare)

**Supporting Files**:
```
.beads/
  ├── beads.db                 # SQLite (primary storage)
  ├── beads.db-wal             # WAL checkpoint file
  ├── beads.db-shm             # Shared memory for concurrent writes
  ├── issues.jsonl             # Git-tracked export
  ├── config.yaml              # Project settings
  ├── routes.jsonl             # Cross-project prefix routing (optional)
  ├── metadata.json            # Workspace metadata
  ├── .br_history/             # Timestamped JSONL backups (auto-rotated)
  └── .manifest.json           # Export manifest (internal)
```

**Event Log** (local DB only):
- `events` table: issue mutations, actor attribution, timestamp
- Never exported to JSONL
- Queryable via CLI: `br audit log <id>`, `br audit summary`
- Indexes on: issue_id, event_type, actor, created_at

---

## CLI Surface

### Core Commands

**Lifecycle**:
```bash
br init                                   # Initialize workspace
br create "Title" --type task -p 1        # Create issue (returns ID)
br q "Quick note"                         # Quick capture (ID only)
br show <id>                              # Show issue + deps + events
br update <id> --status in_progress       # Claim work
br close <id> --reason "Done"             # Mark closed (auto-flushes JSONL)
br reopen <id>                            # Reopen closed issue
br delete <id>                            # Tombstone (soft delete)
br defer <id> --until tomorrow            # Schedule for later
br undefer <id>                           # Make ready again
```

**Querying**:
```bash
br ready                                  # Open, unblocked, not deferred work
br blocked                                # Show blocked issues
br list --status open -p 0-1              # Filter by status/priority
br search "auth"                          # Full-text search
br stale --days 30                        # Find inactive issues
br count --by status                      # Aggregation with grouping
```

**Dependencies**:
```bash
br dep add <child> <parent>               # Mark child blocked by parent
br dep remove <child> <parent>            # Unblock
br dep list <id>                          # Show all deps for issue
br dep tree <id>                          # Visualize dependency graph
br dep cycles                             # Find circular deps (rare)
```

**Metadata**:
```bash
br label add <id> backend auth            # Add labels
br label remove <id> urgent               # Remove labels
br comments add <id> "Note"               # Add comment
br comments list <id>                     # List comments
```

**Planning & Reporting**:
```bash
br epic status --eligible-only            # Epic rollups
br graph <id>                             # ASCII dependency graph
br lint --status all                      # Check template compliance
br orphans                                # Open issues in git commits
br changelog --since-tag v0.1.44          # Generate changelog from closed issues
```

**Sync & Inspection**:
```bash
br sync --flush-only                      # Export DB → JSONL (safe, idempotent)
br sync --status                          # Check sync state
br sync --import-only                     # JSONL → DB (after git pull)
br sync --merge                           # 3-way merge (with base snapshot)
br sync --merge --force-db                # Conflict resolution policies
br history list                           # List timestamped backups
```

**Agent Integration**:
```bash
br ready --json                           # Structured output for agents
br list --json | jq '.issues'
br show <id> --json
br audit record --kind note --actor agent # Record agent actions + metadata
br agents --add --force                   # Inject AGENTS.md instructions
```

**System**:
```bash
br doctor                                 # Diagnostics + repair (--repair)
br stats                                  # Project statistics
br config list / get / set / edit         # Configuration management
br info                                   # Workspace diagnostics
br schema all --format json               # JSON Schema documents
br completions zsh                        # Shell completion setup
br where                                  # Show active .beads directory
br upgrade                                # Self-update (if built with self_update feature)
br version                                # Show version + build info
```

### Global Flags

```bash
--json / --robot                          # Machine-readable JSON output
--quiet / -q                              # Suppress output
--verbose / -v / -vv                      # Increase verbosity (debug)
--no-color                                # Disable ANSI colors
--db <path>                               # Override database path
--allow-external-jsonl                    # Allow JSONL outside .beads/
```

### Environment Variables

```bash
RUST_LOG=error              # Recommended: suppress dependency logs, keep normal output
BD_DB / BD_DATABASE         # Override database path
BEADS_JSONL                 # Override JSONL path (requires --allow-external-jsonl)
BEADS_CACHE_DIR             # Store DB on fast local FS while .beads on network mount
```

---

## Installation & Build

### Quick Install (Recommended)

```bash
# One-liner for most platforms (Linux, macOS, FreeBSD):
curl -fsSL "https://raw.githubusercontent.com/Dicklesworthstone/beads_rust/main/install.sh?$(date +%s)" | bash

# Install flags:
--version vX.Y.Z            # Specific version
--dest ~/.local/bin         # Install location (default)
--system                    # /usr/local/bin (sudo)
--from-source              # Build from source instead of downloading
--skip-skills              # Don't install Claude Code skills
--with-migration-skill     # Add bd-to-br-migration skill
```

### From Source

```bash
# Clone and build (requires Rust 1.88+, nightly)
git clone https://github.com/Dicklesworthstone/beads_rust.git
cd beads_rust
cargo build --release
./target/release/br --help

# Or install globally
cargo install --path .
cargo install --git https://github.com/Dicklesworthstone/beads_rust.git

# With optional MCP server support
cargo install --git ... --features mcp
```

### Release Profile

Optimized for binary distribution:
- `opt-level = "z"` (size optimization)
- Link-time optimization (LTO)
- Single codegen unit
- Panic abort (no unwinding)
- Stripped debug symbols
- Result: 5-8 MB binary (vs 30+ MB original beads)

---

## FreeBSD Compatibility

### Summary

**Status**: ✅ **Highly compatible.** Cargo + Rust nightly works standard on FreeBSD. No platform-specific syscalls or libc oddities detected.

### Platform-Specific Code Review

**fsync/fsync_dir** (`src/util/mod.rs`):
```rust
#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    fs::File::open(path)?.sync_all()  // POSIX fsync, works on FreeBSD
}

#[cfg(not(unix))]
fn sync_directory(_: &Path) -> io::Result<()> {
    // Windows: no portable directory fsync
    tracing::debug!("Skipping parent directory fsync: no portable...");
}
```
- **FreeBSD**: Uses POSIX `fsync()` (via `File::sync_all()`)—fully supported
- No Linux-only syscalls detected

**Symlink Safety** (`src/sync/history.rs`):
```rust
#[cfg(unix)]
use std::os::unix::fs::symlink;

#[test]
#[cfg(unix)]
fn test_list_backups_ignores_symlinked_backup_files() { ... }
```
- **FreeBSD**: Standard POSIX symlinks work identically
- Rejection logic: symlinks pointing outside `.beads/` are blocked (safety, not compat)

**File Permissions** (`src/util/mod.rs`):
```rust
#[cfg(unix)]
{
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);  // Owner read+write only
}
```
- **FreeBSD**: POSIX `mode_t` and `open()` flags work identically to Linux

**Git Integration**:
- No runtime `Command::new("git")` in sync paths (explicit safety invariant)
- Reporting commands (`br changelog`, `br orphans`, `br stats`) may spawn `git log` for historical analysis
- All git operations remain **user-controlled** (br never auto-commits/pushes)

### Verified Absence of Linux-Only Patterns

✓ No `libc` crate (would force Linux/glibc assumptions)  
✓ No `procfs` or `/proc/*` inspection  
✓ No `epoll` / `kqueue` distinction (async runtime uses standard Rust futures)  
✓ No special `/sys/` or sysctl handling  
✓ No `systemd` or platform-specific service files included  
✓ No Linux-specific shell scripts in install.sh (detects platform, downloads appropriate binary)  

### Build Instructions for FreeBSD

```bash
# Install Rust (standard process on FreeBSD)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Activate Rust nightly
rustup default nightly

# Clone and build
git clone https://github.com/Dicklesworthstone/beads_rust.git
cd beads_rust
cargo build --release

# Binary ready at: ./target/release/br
./target/release/br --version
./target/release/br init
```

### Known Caveats

1. **Nightly Rust required**: Cargo.toml specifies `edition = "2024"` (nightly only)
   - FreeBSD ports may not have latest nightly
   - Rustup handles this seamlessly on all BSDs

2. **Binary download script**: Install script downloads platform-specific binaries for Linux/macOS/Windows only
   - Workaround: Build from source (`--from-source` flag) or use `cargo install`
   - Script itself is portable (detects platform, falls back gracefully)

3. **No FreeBSD-specific binary releases yet**
   - Can be added to GitHub Actions CI pipeline if demand exists
   - Source builds work identically

---

## Multi-User & Team Workflows

### Git-Based Async Collaboration

br assumes **async, git-based collaboration**:

1. Developer A works in their local `.beads/` (SQLite DB, not committed)
2. A exports via `br sync --flush-only` → `.beads/issues.jsonl` (committed to git)
3. Developer B pulls the repo, runs `br sync --import-only` → loads JSONL into their SQLite DB
4. B makes changes locally, exports, commits
5. A pulls, imports, continues

**Conflict Resolution** (`br sync --merge`):
- Detects if both sides changed the same issue
- Offers 3 resolution policies:
  - `--force-db`: Keep local SQLite version
  - `--force-jsonl`: Keep JSONL version
  - `--force`: Keep newer timestamp
- Uses `.beads/beads.base.jsonl` as common ancestor (3-way merge)

### Cross-Project Routing

For **monorepos** with multiple `.beads/` directories:

```jsonl
# .beads/routes.jsonl
{"prefix":"api-","path":"../api"}
{"prefix":"ops-","path":"/srv/projects/ops/.beads"}
```

Route-aware commands (`show`, `update`, `close`, `dep`, `audit`) operate on external issues without copying them:
- Acquires write lock in target workspace
- Updates target workspace storage
- Still does **not** sync repos (git push/pull is user's responsibility)

### Attribution & Actor Tracking

Every mutation records the actor:

```bash
br update br-123 --status in_progress --actor alice@example.com
br close br-123 --actor alice@example.com --reason "Done"

# Or default from git config:
br update br-123 --status in_progress  # uses `git config user.email`
```

**Event Log**:
```bash
br audit log br-123      # Show all changes to issue, with actors
br audit summary --days 30  # Activity by actor over period
br audit record --kind note --actor agent --issue br-123 "Some data"
```

---

## Integration with AI Agents

### JSON Output for Structured Processing

Every query command supports `--json`:

```bash
# Agents can parse structured output
br ready --json | jq '
  .issues[] |
  select(.priority <= 1) |
  {id, title, assignee, status}
'

br list --json --status open -p 0-1 | jq '.total'
br show br-123 --json | jq '.issue | {id, title, deps}'
```

### Agent Workflow Pattern

**Typical agent integration** (e.g., Claude Code, Codex):

```bash
# 1. Find actionable work
br ready --json | jq -r '.issues[0].id' > CURRENT_ISSUE

# 2. Claim it
ISSUE=$(cat CURRENT_ISSUE)
br update $ISSUE --status in_progress --actor "claude-agent"

# 3. Work on it (code changes, etc.)
# ... implement fix ...

# 4. Record progress
br comments add $ISSUE "Implemented X, testing Y"

# 5. Close when done
br close $ISSUE --actor "claude-agent" --reason "Implemented + tested"

# 6. Sync to git
br sync --flush-only
git add .beads/ && git commit -m "Close: $ISSUE"
```

### AGENTS.md Injection

`br agents --add --force` injects workflow instructions into `AGENTS.md` or `CLAUDE.md`:

```markdown
<!-- br-agent-instructions-v1 -->
## Beads Workflow Integration

Essential commands:
- `br ready` — find actionable work
- `br show <id>` — issue details + deps
- `br create ... --type task --priority 1` — new issue
- `br close <id> --reason "Implemented"` — mark done
- `br sync --flush-only` — export to JSONL before session end
```

### Persistent Memory

**Audit trail** provides queryable history:
- Event table: issue_id, event_type (Created, Updated, Closed, Commented), actor, timestamp
- Indexed for fast lookup
- Can be queried programmatically for agent feedback loops

**Example: Agent self-awareness**:
```bash
# "How many issues have I completed this week?"
br audit summary --days 7 --actor claude-agent | jq '.actors[] | select(.actor == "claude-agent")'

# "What did I change on this issue?"
br audit log br-123 --json | jq '.events[] | select(.actor == "claude-agent")'
```

### MCP Server Support (Optional)

With `--features mcp`, run `br serve --actor <name>` for AI agents to communicate via Model Context Protocol:

```bash
RUST_LOG=error br serve --actor claude-codex
```

- Agents discover tools via MCP introspection (rather than shelling out)
- Uses same SQLite DB, JSONL export, write locks as CLI
- Preferred for agents in continuous environments

---

## C-137 (AI Agent Infrastructure) Intersections

### 1. Persistent Memory / Query Surface

✅ **Supported**:
- Event table provides structured audit trail
- Indexed by issue_id, actor, event_type, timestamp
- Queryable via `br audit log` and programmatic JSON
- Natural fit for agent feedback loops ("How did I do last week?")

**Gap**: No global query language (you export to JSON, then post-process)

### 2. Multi-Peer Dialogue

⚠️ **Partial**:
- **Actor attribution**: Every change records the agent name
- **Async collaboration**: Git-based JSONL export enables multi-agent workflows
- **No real-time messaging**: br is async/git-based; no message bus or notifications
- **Workaround**: Combine with separate notification layer (e.g., MCP Agent Mail)

### 3. Lifecycle Observability

✅ **Excellent**:
- Event log is the lifecycle record
- Can be queried: `br audit log <id>` returns full change history
- CLI: `br stats` for aggregate metrics
- Machine-readable: `--json` on all queries

### 4. Identity Continuity

✅ **Strong**:
- Actor field on every event: agent name, email, or ID
- Audit commands: `br audit summary --actor <name>`
- Attribution is explicit and immutable in event log

---

## Adoption Suitability for FreeBSD-15

### ✅ Recommended: Install Today

**Minimal effort**:
```bash
# Option 1: Build from source (5 minutes)
git clone https://github.com/Dicklesworthstone/beads_rust.git
cd beads_rust && cargo build --release
sudo install -m 0755 target/release/br /usr/local/bin/

# Option 2: Cargo install (also 5 minutes, deps handled)
cargo install --git https://github.com/Dicklesworthstone/beads_rust.git

# Verify
br --version && br init  # Start using immediately
```

**Why now**:
- Rust 1.88+ nightly is stable on FreeBSD
- No platform-specific friction detected
- Pure-Rust SQLite (fsqlite) handles all DB I/O
- Zero dependencies on Linux-isms or systemd

**Why wait (optional)**:
- If you prefer pre-built FreeBSD binaries (could be added to GitHub releases)
- If your FreeBSD ports tree is stale (use rustup instead)

### Minimum Setup

```bash
# 1. Install br
cargo install --git https://github.com/Dicklesworthstone/beads_rust.git

# 2. Initialize in your project
cd ~/projects/c-137
br init

# 3. Create first issue
br create "Design agent memory layer" --type feature --priority 1

# 4. Start using (dev workflow)
br ready                    # Find work
br update br-abc --status in_progress
# ... code ...
br close br-abc --reason "Complete"
br sync --flush-only && git add .beads && git commit "Close: br-abc"

# 5. Later: CI integration
# Add to build scripts if agents should auto-create issues on failure:
br create "CI failure: $TEST_SUITE" --type bug --priority 0 --description "..."
```

### Integration with C-137

For **agent infrastructure**, br serves as:

1. **Issue repository**: Persistent, queryable, versioned
2. **Agent work queue**: `br ready --json` for dispatcher
3. **Audit log**: Event table for observability + feedback loops
4. **Collaboration ledger**: Multi-agent JSONL sync via git

**Not a fit for**:
- Real-time collaboration (use git + MCP Agent Mail instead)
- System-wide event bus (design a separate MCP service)
- Distributed consensus (br is local-first, one DB per repo)

---

## Limitations & Non-Features

br **intentionally does not** support:

| Feature | Reason |
|---------|--------|
| Automatic git commits | Non-invasive philosophy; user controls VCS |
| Git hook installation | User-controlled; add manually if desired |
| Background daemon | Simple CLI, no processes; use MCP for long-running agents |
| Dolt backend | SQLite + JSONL only (frozen at classic architecture) |
| Linear/Jira sync | Focused scope; sync with git, not SaaS |
| Web UI | CLI-first (see beads_viewer for TUI) |
| Automatic multi-repo sync | Routes are local dispatch tables; git sync is explicit |
| Real-time notifications | Async/git-based; pair with separate message service |

---

## References

- **Repo**: https://github.com/Dicklesworthstone/beads_rust (MIT + OpenAI/Anthropic Rider)
- **Original beads**: https://github.com/steveyegge/beads (Go; diverged, now evolving to GasTown)
- **beads_viewer (bv)**: https://github.com/Dicklesworthstone/beads_viewer (TUI companion)
- **MCP Agent Mail**: https://github.com/Dicklesworthstone/mcp_agent_mail (messaging for agents)
- **Rust edition**: 2024 (nightly required)
- **Rust version**: 1.88+
- **License**: MIT (with OpenAI/Anthropic Rider)

---

## Quick Glossary

- **br**: Command name (beads_rust CLI)
- **bd**: Original beads command (compatibility reference)
- **`.beads/`**: Project issue workspace (git-tracked, except `.beads/.br_history/`)
- **JSONL**: Line-delimited JSON; one issue per line; merges cleanly in git
- **WAL**: Write-Ahead Log (SQLite concurrency mode)
- **Ready**: Open, unblocked, not deferred; actionable work
- **Blocked**: Waiting on dependency
- **Actor**: Agent, user, or identifier making a change
- **Event**: Immutable record in audit trail (Created, Updated, Closed, Commented)
- **Route**: Cross-project prefix dispatch (for monorepos)
- **Sync**: Export DB to JSONL or import JSONL to DB
- **Flush**: Export (DB → JSONL); `--flush-only` is idempotent and safe


---

## Fork operating policy (read this before doing anything PR-shaped)

This is **not our project** — it's a fork of Dicklesworthstone/beads_rust, found
via the flywheel ecosystem. The operating rules, settled 2026-06 after experience:

- **Never file PRs upstream.** The upstream author does not accept them: PRs get
  reviewed, the idea reimplemented in-house, and the PR closed. Don't spend the
  effort.
- **We commit directly to our own trunk** (`main` on sahuagin/beads_rust). No
  branch-and-PR ceremony in this repo — that convention is for repos we own the
  review loop on.
- **We do sync/merge upstream occasionally** (see the `Merge branch
  'Dicklesworthstone:main' into flywheel` commits). Keep fork-local changes in
  fork-local files (like this one) or cleanly-separable commits to keep those
  merges cheap. AGENTS.md is upstream's file — don't edit it.
- **Issues for br itself go in this repo's own `.beads/`** (GitHub issues are
  disabled). Example: `beads_rust-qqtx` (status-validation bug, 2026-06-07).

### Strategic position (operator, 2026-06-07)

We use br for two features: **dependency tracking** and **priority promotion**.
No allegiance beyond that. Known tradeoffs: the original beads (Go, Steve Yegge)
has more capability and fewer core dumps, but upstream never responded to an
offered priority-inversion fix, and br's non-invasiveness (no daemons, no
auto-commits, no hooks) fits the flywheel toolchain better. If br's maintenance
cost rises or the Go original becomes responsive, this choice is revisitable —
nothing in our tooling assumes br beyond the `br` CLI surface and the
`.beads/issues.jsonl` export format.
