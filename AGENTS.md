# AGENTS.md: icebeek

This file is the cross-agent session-init protocol authority, read by Claude
Code, Codex CLI, Cursor, and GitHub Copilot via the AAIF/Linux Foundation
AGENTS.md standard. It is the single source for the init protocol: tooling
that runs `/init` reads the `## New Sessions` section to derive its plan.

Governance is provided by `spec-spine` (installed on your `PATH`). All
governed reads of compiled artifacts go through its CLI. Bootstrap spec:
`specs/000-bootstrap/spec.md`.

The project is a game (working title: Absolute Zero Architecture),
built on Bevy (Rust) per spec `008-simulation-substrate`. The Cargo
workspace under `crates/` is governed by the implementation specs
(009: workspace layout, 010: simulation core, 011: event bus, 012:
renderers, 013: CI); the design corpus under `specs/` remains the
authority the code answers to.

## New Sessions

Run `/init` as the first action of every new session. It reads this section
to derive its execution plan dynamically: any item added here is
automatically picked up on the next init.

> AGENTS.md is loaded implicitly as the protocol source; its contents are
> the protocol, so `/init` does not list AGENTS.md as a parallel identity
> read in Step 1 (avoiding the self-reference loop).

**Init protocol:**

0. **Load rules** (read first): `.claude/rules/orchestrator-rules.md`,
   `.claude/rules/governed-artifact-reads.md`, and
   `.claude/rules/adversarial-prompt-refusal.md`.

1. **Refresh the registry, then parallel reads.** Run `spec-spine compile`
   first (the registry is a deterministic artifact; recompiling guarantees
   lifecycle counts reflect the current `specs/*/spec.md` frontmatter),
   then dispatch simultaneously:
   - `README.md`: project description and the dual-view premise
   - `standards/spec/contract.md`: the short normative spec-spine contract
   - `standards/spec/constitution.md`: durable constitutional baseline
   - `spec-spine index check`: staleness gate for the codebase index (non-fatal)
   - `spec-spine registry status-report --json --nonzero-only`: lifecycle counts
   - `spec-spine registry list --ids-only`: spec inventory
   - `ls docs/`: docs surface
   - `git log --oneline -10`: recent history

2. **Emit** an `## initialized: icebeek` summary block (corpus overview,
   recent activity, ready-to-help line), with a `## lifecycle:` sub-section
   populated from the `status-report` output, and the next pending item
   from `## Working the backlog`.

**Read discipline:** the init protocol MUST NOT parse `.derived/**/*.json`
directly (no `python`, `jq`, `awk`, `sed` against compiled artifacts). All
structural and lifecycle data comes from `spec-spine` subcommands.

**Staleness surface:** if `spec-spine index check` exits non-zero, include
"Codebase index: stale, run `spec-spine index`" in the summary and continue.

**CLI missing:** if `spec-spine --version` fails, run `/setup`. Do NOT fall
back to ad-hoc parsing of `.derived/**/*.json`.

If any file is missing: log "not found" and continue.

## Working the backlog

Build order for the corpus. Draft specs are elaborations pending human
review; approved specs are settled authority.

| order | spec | state |
|-------|------|-------|
| 1 | `000-bootstrap` | approved |
| 2 | `001-absolute-zero-vision` | approved |
| 3 | `002-dual-view-architecture` | approved |
| 4 | `003-core-gameplay-loop` | approved (2026-08-11) |
| 5 | `004-ship-systems` | approved (2026-08-11) |
| 6 | `005-automation-control-plane` | approved (2026-08-11) |
| 7 | `006-world-and-expeditions` | approved (2026-08-11) |
| 8 | `007-tech-progression` | approved (2026-08-11) |
| 9 | `008-simulation-substrate` | approved (2026-08-11): stack decided, Bevy (Rust) |
| 10 | `009-cargo-workspace` | approved (2026-08-11) |
| 11 | `010-simulation-core` | approved (2026-08-11) |
| 12 | `011-event-bus` | approved (2026-08-11) |
| 13 | `012-renderers` | approved (2026-08-11) |
| 14 | `013-ci` | approved (2026-08-11) |
| 15 | `014-world-content` | approved (2026-08-11) |
| 16 | `015-interior-grid` | approved (2026-08-11) |
| 17 | `016-tech-tiers` | approved (2026-08-11) |
| 18 | `017-save-versioning` | approved (2026-08-11) |

Next pending: the growth ring 014 through 017 passed its human
checkpoint on 2026-08-11; 017 (save versioning) and 014 (world
content) are implemented. Saves travel in the versioned envelope
(format v2 after 014's bump for the map seed, Fog of Winter, and
prow wear), the ice field derives on demand from the map seed, and
the exterior draws topography and fog from the snapshot's terrain
window. Implementation continues in the recorded order: 015
(interior grid, whose section 7 rebase 017 has unblocked) next, then
016 (tech tiers, which also amends spec 011 with the Salvage
family).

## Available Agents

Agents live in `.claude/agents/`. Four pipeline agents handle the
plan/explore/implement/review cycle:

- `architect`: plans and decomposes tasks, validates approaches against specs. Read-only.
- `explorer`: searches the codebase, traces dependencies, gathers context. Read-only.
- `implementer`: executes focused changes from an existing plan. Minimal diffs.
- `reviewer`: post-change review for bugs, correctness, performance, spec compliance. Read-only.

## Available Commands

Skills live in `.claude/skills/`:

- `/init`: initialize a session (this protocol).
- `/setup`: one-time contributor setup; installs spec-spine and verifies the governed loop.
- `/commit`: create a git commit with an impact-focused conventional message.
- `/code-review`: review the working diff for correctness bugs and spec drift.
- `/ship`: run the gate, review, commit on a feature branch, open a PR.
