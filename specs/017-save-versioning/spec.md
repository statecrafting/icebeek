---
id: "017-save-versioning"
title: "Save versioning: the envelope, migration chains, refusal"
status: draft
created: "2026-08-11"
depends_on:
  - "008-simulation-substrate"
  - "010-simulation-core"
  - "011-event-bus"
establishes:
  - "docs/design/save-versioning.md"
summary: >
  The policy spec 010 section 7 deferred: a versioned save envelope
  (format version, crate version, TICK_HZ) wrapping the state payload;
  a compatibility rule of load-same, migrate-older-when-a-chain-exists,
  refuse-newer, refuse-unknown, always with a typed error naming the
  versions; and migration as a chain of pure, total, deterministic
  functions from version N to N+1, exercised against committed golden
  fixture saves. The event schema is save surface (spec 011 section 6),
  so variant renames and removals ride format bumps here instead of
  breaking silently. Until a migration is authored, a bump refuses old
  saves loudly; corrupting or reinterpreting them is never an option.
---

# 017: Save versioning

## 1. Purpose

Spec 010 §7 ships saves that carry the crate version and TICK_HZ and
may refuse a mismatch, and defers versioning and migration policy to
a future spec. This is that spec: the envelope, the compatibility
rules, and the migration mechanism that lets the state schema grow
(spec 015 §7 already queues a breaking rebase) without stranding
players or corrupting runs.

## 2. The envelope

A save file is an envelope wrapping the serialized state payload:

- **`format_version`**: a monotonically increasing integer owned by
  this spec; every schema-visible change to `SaveState`, including
  event enum changes (spec 011 §6), increments it.
- **`crate_version`** and **`tick_hz`**: retained from spec 010 §7
  as diagnostics; `tick_hz` mismatch remains a refusal until a
  migration explicitly converts it.
- The envelope is stable across all future format versions: any
  build can always read the envelope of any save and name its
  version in an error.

## 3. Compatibility rules

On load, exactly one of:

1. **Same `format_version`**: load directly.
2. **Older, with a complete migration chain to current**: migrate,
   then load. The player may be informed; the migration itself is
   silent and total.
3. **Older, chain incomplete**: refuse with a typed error naming
   the save's version, the current version, and the missing step.
4. **Newer than current**: refuse, naming both versions (no forward
   loading, ever).
5. **Unreadable envelope**: refuse as corrupt.

Refusal is always loud and typed; silent reinterpretation of old
bytes is forbidden in the same spirit as spec 010 §7's tick-rate
refusal.

## 4. Migration chains

- A migration is a **pure, total, deterministic** function from the
  version-N payload model to version N+1. No I/O, no RNG, no
  wall-clock; given the same input save it yields the same output
  save, byte for byte.
- Chains compose stepwise: N to current runs N→N+1→…; no skip-level
  migrations, so each step is written and tested once.
- A migration may synthesize defaults for new state (a new domain
  starts at its fresh-run default) and must document any gameplay
  visible consequence in the migration's doc comment.
- Authoring a format bump **without** a migration is legal: rule 3
  then refuses old saves. The bump PR must say so explicitly; the
  refusal is a product decision made in review, not an accident.

## 5. Test contract

Headless, in the sim crate's suite:

1. **Golden fixtures.** Each released format version keeps a
   committed fixture save; the chain migrates every fixture to
   current and the result loads and ticks deterministically.
2. **Refusal matrix.** Newer, unknown, chain-gap, and corrupt saves
   each produce their typed error, never a panic or partial load.
3. **Migration determinism.** Migrating the same fixture twice
   yields byte-identical output.
4. **Envelope stability.** The envelope of every fixture, oldest to
   newest, parses with the current reader.

## 6. Out of scope

- Autosave cadence, save slots, cloud sync, save-file UI: app-shell
  concerns for a future spec (spec 012 §5 owns orchestration).
- Compression and encryption of the payload: future spec if wanted.
- Replay-file format (seed plus command log): related but separate;
  future spec.
