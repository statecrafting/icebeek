# Save versioning

<!-- Normative authority: specs/017-save-versioning/spec.md -->

Every save is an envelope (format version, crate version, tick rate)
around the state payload, and the envelope is forever: any build can
read any save's envelope and name its version in an error.

Loading follows one rule chain: same version loads; an older save
with a complete migration chain migrates stepwise (N to N+1, each
step a pure, total, deterministic function exercised against
committed golden fixtures); anything else refuses loudly with a
typed error naming both versions. Newer saves never load. Corrupt
envelopes never guess.

The event schema is save surface, so renaming or removing an event
variant is a format bump here, not a quiet edit. Shipping a bump
without a migration is allowed but must be said out loud in the PR:
refusing old saves is a product decision made in review, never an
accident of serialization.
