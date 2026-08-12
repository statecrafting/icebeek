//! The save envelope, compatibility rules, and migration chains
//! (spec 017). Every save on disk is an envelope naming its format
//! version around the serialized state payload; the envelope is
//! stable across all future format versions, so any build can read
//! any save's envelope and name its version in a typed error.
//! Loading follows spec 017 section 3 exactly: the same version
//! loads, an older version migrates when a complete stepwise chain
//! exists, and everything else refuses loudly. Silent
//! reinterpretation of old bytes is forbidden.

use serde::Serialize;
use serde_json::Value;

use crate::{SaveState, TICK_HZ};

/// The current save format version (spec 017 section 2): a
/// monotonically increasing integer owned by spec 017. Every
/// schema-visible change to [`SaveState`], including event enum
/// changes (spec 011 section 6), increments this by one and either
/// appends the matching step to `MIGRATIONS` or states in the bump
/// PR that older saves are now refused (spec 017 section 4).
pub const SAVE_FORMAT_VERSION: u32 = 2;

/// Typed load refusals (spec 017 section 3). Every branch names the
/// versions involved; no load path panics or partially applies.
#[derive(Debug)]
pub enum SaveError {
    /// The envelope did not parse: not JSON, or missing one of its
    /// stable fields (spec 017 section 3 rule 5).
    CorruptEnvelope(String),
    /// The save was written by a newer build; forward loading never
    /// happens (spec 017 section 3 rule 4).
    NewerFormat { saved: u32, current: u32 },
    /// The save is older and the stepwise chain has no migration
    /// from `missing` to `missing + 1` (spec 017 section 3 rule 3).
    MissingMigration {
        saved: u32,
        current: u32,
        missing: u32,
    },
    /// The save was written under a different tick rate and no
    /// migration converted it (spec 010 section 7).
    TickRateMismatch { saved: u32, expected: u32 },
    /// The envelope was sound but the payload did not decode.
    Decode(String),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::CorruptEnvelope(err) => write!(f, "save envelope unreadable: {err}"),
            SaveError::NewerFormat { saved, current } => write!(
                f,
                "save format v{saved} is newer than this build's v{current}; \
                 forward loading never happens"
            ),
            SaveError::MissingMigration {
                saved,
                current,
                missing,
            } => write!(
                f,
                "save format v{saved} cannot reach v{current}: no migration from v{missing} \
                 to v{}",
                missing + 1
            ),
            SaveError::TickRateMismatch { saved, expected } => {
                write!(
                    f,
                    "save written at {saved} Hz, this build runs {expected} Hz"
                )
            }
            SaveError::Decode(err) => write!(f, "save payload decode failed: {err}"),
        }
    }
}

impl std::error::Error for SaveError {}

/// One step of a migration chain (spec 017 section 4): a pure,
/// total, deterministic function from the version-`from` envelope to
/// the version-`from + 1` envelope. Steps compose stepwise from N to
/// current; there are no skip-level migrations. A step may rewrite
/// the payload and the diagnostic fields (a tick-rate conversion is
/// legal here) but never `format_version`, which the chain walker
/// owns. Any gameplay-visible consequence, such as a synthesized
/// fresh-run default for new state, belongs in the step's doc
/// comment.
pub struct Migration {
    /// The format version this step consumes; it emits `from + 1`.
    pub from: u32,
    pub run: fn(Value) -> Result<Value, String>,
}

/// The authored chain, one step per released format bump.
static MIGRATIONS: &[Migration] = &[Migration {
    from: 1,
    run: migrate_v1_to_v2,
}];

/// v1 to v2 (spec 014, world content): the world domain gains
/// `map_seed` and the Fog of Winter reveal set, and ship kinetics
/// gains `prow_wear`. All three are synthesized fresh-run defaults,
/// with gameplay-visible consequences: a migrated run's ice field
/// materializes from map seed 0 (v1 never recorded a map seed), its
/// map starts unexplored beyond what re-reveals around the ship, and
/// its prow starts unworn.
fn migrate_v1_to_v2(mut envelope: Value) -> Result<Value, String> {
    let payload = envelope
        .get_mut("payload")
        .ok_or("v1 save has no payload")?;
    let world = payload
        .get_mut("world")
        .and_then(Value::as_object_mut)
        .ok_or("v1 save has no world domain")?;
    world.insert("map_seed".into(), 0u64.into());
    world.insert("fog".into(), serde_json::json!({ "revealed": [] }));
    let kinetics = payload
        .get_mut("kinetics")
        .and_then(Value::as_object_mut)
        .ok_or("v1 save has no kinetics domain")?;
    kinetics.insert("prow_wear".into(), 0.0f32.into());
    Ok(envelope)
}

/// The write-side envelope (spec 017 section 2). `crate_version` and
/// `tick_hz` are diagnostics; `format_version` is the authority.
#[derive(Serialize)]
struct EnvelopeOut<'a> {
    format_version: u32,
    crate_version: &'a str,
    tick_hz: u32,
    payload: &'a SaveState,
}

pub(crate) fn encode(save: &SaveState) -> Vec<u8> {
    serde_json::to_vec(&EnvelopeOut {
        format_version: SAVE_FORMAT_VERSION,
        crate_version: env!("CARGO_PKG_VERSION"),
        tick_hz: TICK_HZ,
        payload: save,
    })
    .expect("save state serializes")
}

pub(crate) fn decode(bytes: &[u8]) -> Result<SaveState, SaveError> {
    decode_with(bytes, MIGRATIONS, SAVE_FORMAT_VERSION)
}

/// The full load cascade of spec 017 section 3, parameterized over
/// the chain and the current version so tests can drive synthetic
/// chains through the same path production uses.
fn decode_with(bytes: &[u8], chain: &[Migration], current: u32) -> Result<SaveState, SaveError> {
    let envelope: Value =
        serde_json::from_slice(bytes).map_err(|e| SaveError::CorruptEnvelope(e.to_string()))?;
    let saved = read_envelope(&envelope)?;
    if saved > current {
        return Err(SaveError::NewerFormat { saved, current });
    }
    let envelope = migrate(envelope, saved, current, chain)?;
    // Post-migration on purpose: a chain step may explicitly convert
    // the tick rate; without one, the mismatch refuses (spec 010
    // section 7, spec 017 section 2).
    let tick_hz = field_u32(&envelope, "tick_hz")?;
    if tick_hz != TICK_HZ {
        return Err(SaveError::TickRateMismatch {
            saved: tick_hz,
            expected: TICK_HZ,
        });
    }
    let payload = field(&envelope, "payload")?.clone();
    serde_json::from_value(payload).map_err(|e| SaveError::Decode(e.to_string()))
}

/// Validate the stable envelope surface and return `format_version`.
/// This is the part of a save every build can always read (spec 017
/// section 2).
fn read_envelope(envelope: &Value) -> Result<u32, SaveError> {
    if !field(envelope, "crate_version")?.is_string() {
        return Err(SaveError::CorruptEnvelope(
            "envelope field `crate_version` is not a string".into(),
        ));
    }
    field(envelope, "payload")?;
    field_u32(envelope, "tick_hz")?;
    field_u32(envelope, "format_version")
}

fn field<'a>(envelope: &'a Value, name: &str) -> Result<&'a Value, SaveError> {
    envelope
        .get(name)
        .ok_or_else(|| SaveError::CorruptEnvelope(format!("envelope is missing `{name}`")))
}

fn field_u32(envelope: &Value, name: &str) -> Result<u32, SaveError> {
    field(envelope, name)?
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| {
            SaveError::CorruptEnvelope(format!("envelope field `{name}` is not a small integer"))
        })
}

/// Walk the stepwise chain from `saved` to `current` (spec 017
/// section 4). Total when the chain is complete; a hole refuses with
/// the exact missing step.
fn migrate(
    mut envelope: Value,
    saved: u32,
    current: u32,
    chain: &[Migration],
) -> Result<Value, SaveError> {
    for version in saved..current {
        let step =
            chain
                .iter()
                .find(|step| step.from == version)
                .ok_or(SaveError::MissingMigration {
                    saved,
                    current,
                    missing: version,
                })?;
        envelope = (step.run)(envelope).map_err(|e| {
            SaveError::CorruptEnvelope(format!(
                "migration v{version} to v{} rejected the save: {e}",
                version + 1
            ))
        })?;
    }
    Ok(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Condition, RuleAction, SimWorld};

    /// Each released format version keeps a committed fixture save
    /// (spec 017 section 5 test 1). Released fixtures are frozen:
    /// never regenerate an old version's file.
    const FIXTURES: &[(u32, &str)] = &[
        (1, include_str!("../fixtures/format-v1.json")),
        (2, include_str!("../fixtures/format-v2.json")),
    ];

    /// A deterministic scripted run rich enough that every domain
    /// serializes something interesting.
    fn fixture_run() -> SimWorld {
        let mut sim = SimWorld::new(20260811);
        sim.push_command(Command::SetThrottle { throttle: 1.0 });
        sim.push_command(Command::SetHeading { heading_rad: 0.3 });
        sim.push_command(Command::AddRule {
            condition: Condition::FuelBufferBelow(2.0),
            action: RuleAction::SetFeedEnabled(true),
        });
        for _ in 0..240 {
            sim.tick();
        }
        sim
    }

    fn current_envelope() -> Value {
        serde_json::from_slice(&fixture_run().save_bytes()).expect("envelope parses")
    }

    /// Writes the CURRENT format's golden fixture. Run explicitly on
    /// a format bump, once, and commit the new file:
    /// `cargo test -p icebeek-sim -- --ignored write_current_fixture`
    #[test]
    #[ignore = "writes fixtures/format-v<current>.json; run on format bumps only"]
    fn write_current_fixture() {
        let path = format!(
            "{}/fixtures/format-v{SAVE_FORMAT_VERSION}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::write(path, fixture_run().save_bytes()).expect("fixture written");
    }

    /// Spec 017 section 5 test 1: the chain migrates every fixture
    /// to current, and the result loads and ticks deterministically.
    #[test]
    fn golden_fixtures_load_and_tick_deterministically() {
        for (version, fixture) in FIXTURES {
            let run = |bytes: &[u8]| {
                let mut sim = SimWorld::from_save_bytes(bytes)
                    .unwrap_or_else(|e| panic!("fixture v{version} refused: {e}"));
                for _ in 0..60 {
                    sim.tick();
                }
                sim.save_bytes()
            };
            assert_eq!(
                run(fixture.as_bytes()),
                run(fixture.as_bytes()),
                "fixture v{version} ticked nondeterministically"
            );
        }
    }

    /// Spec 017 section 5 test 4: the envelope of every fixture,
    /// oldest to newest, parses with the current reader and names
    /// its version.
    #[test]
    fn envelope_of_every_fixture_parses() {
        for (version, fixture) in FIXTURES {
            let envelope: Value = serde_json::from_str(fixture).expect("fixture is JSON");
            assert_eq!(read_envelope(&envelope).expect("envelope reads"), *version);
        }
    }

    /// Spec 017 section 3 rule 4: newer saves refuse, naming both
    /// versions.
    #[test]
    fn newer_save_refused_naming_both_versions() {
        let mut envelope = current_envelope();
        envelope["format_version"] = (SAVE_FORMAT_VERSION + 1).into();
        let bytes = serde_json::to_vec(&envelope).unwrap();
        match SimWorld::from_save_bytes(&bytes) {
            Err(SaveError::NewerFormat { saved, current }) => {
                assert_eq!(saved, SAVE_FORMAT_VERSION + 1);
                assert_eq!(current, SAVE_FORMAT_VERSION);
            }
            Err(other) => panic!("wrong error: {other:?}"),
            Ok(_) => panic!("a newer save loaded"),
        }
    }

    /// Spec 017 section 3 rule 3: an older save with a hole in the
    /// chain refuses, naming the missing step. Format version 0 was
    /// never released, so nothing migrates from it.
    #[test]
    fn chain_gap_refused_naming_the_missing_step() {
        let mut envelope = current_envelope();
        envelope["format_version"] = 0.into();
        let bytes = serde_json::to_vec(&envelope).unwrap();
        match SimWorld::from_save_bytes(&bytes) {
            Err(SaveError::MissingMigration {
                saved,
                current,
                missing,
            }) => {
                assert_eq!(saved, 0);
                assert_eq!(current, SAVE_FORMAT_VERSION);
                assert_eq!(missing, 0);
            }
            Err(other) => panic!("wrong error: {other:?}"),
            Ok(_) => panic!("a chain-gapped save loaded"),
        }
    }

    /// Spec 017 section 3 rule 5 and section 5 test 2: corrupt saves
    /// refuse as corrupt, never panic or partially load. The bare
    /// pre-envelope format has no `format_version`, so it lands here
    /// too.
    #[test]
    fn corrupt_saves_refused_not_panicked() {
        for bytes in [
            &b"not a save"[..],
            &b"{}"[..],
            &b"{\"format_version\": \"one\", \"crate_version\": \"0.1.0\"}"[..],
            &b"{\"format_version\": 1, \"crate_version\": \"0.1.0\", \"tick_hz\": 20}"[..],
        ] {
            match SimWorld::from_save_bytes(bytes) {
                Err(SaveError::CorruptEnvelope(_)) => {}
                Err(other) => panic!("wrong error: {other:?}"),
                Ok(_) => panic!("corrupt bytes loaded"),
            }
        }
    }

    /// Spec 010 section 7, held at the envelope: a tick-rate
    /// mismatch refuses unless a migration explicitly converts it.
    #[test]
    fn tick_rate_mismatch_refused() {
        let mut envelope = current_envelope();
        envelope["tick_hz"] = (TICK_HZ + 1).into();
        let bytes = serde_json::to_vec(&envelope).unwrap();
        match SimWorld::from_save_bytes(&bytes) {
            Err(SaveError::TickRateMismatch { saved, expected }) => {
                assert_eq!(saved, TICK_HZ + 1);
                assert_eq!(expected, TICK_HZ);
            }
            Err(other) => panic!("wrong error: {other:?}"),
            Ok(_) => panic!("expected TickRateMismatch"),
        }
    }

    // A synthetic chain for exercising the walker without touching
    // the production table: each step stamps its passage into the
    // crate_version diagnostic.
    fn stamp_one(mut envelope: Value) -> Result<Value, String> {
        envelope["crate_version"] = "migrated-by-step-one".into();
        Ok(envelope)
    }
    fn stamp_two(mut envelope: Value) -> Result<Value, String> {
        let seen = envelope["crate_version"].as_str().unwrap_or("").to_string();
        envelope["crate_version"] = format!("{seen},then-step-two").into();
        Ok(envelope)
    }
    const SYNTHETIC: &[Migration] = &[
        Migration {
            from: 1,
            run: stamp_one,
        },
        Migration {
            from: 2,
            run: stamp_two,
        },
    ];

    /// Spec 017 section 5 test 3 and section 4: a complete chain
    /// runs stepwise, in order, and migrating the same save twice
    /// yields byte-identical output.
    #[test]
    fn complete_chain_migrates_stepwise_and_deterministically() {
        let envelope = current_envelope();
        let walk = || {
            let migrated = migrate(envelope.clone(), 1, 3, SYNTHETIC).expect("chain complete");
            serde_json::to_vec(&migrated).unwrap()
        };
        let first = walk();
        assert_eq!(first, walk(), "migration is not deterministic");
        let migrated: Value = serde_json::from_slice(&first).unwrap();
        assert_eq!(
            migrated["crate_version"], "migrated-by-step-one,then-step-two",
            "steps ran out of order or skipped"
        );
    }

    /// Spec 017 section 3 rule 2: an older save with a complete
    /// chain migrates and then loads through the normal cascade.
    #[test]
    fn migrated_save_loads_end_to_end() {
        let bytes = fixture_run().save_bytes();
        let state = decode_with(&bytes, SYNTHETIC, 3).expect("older save migrates and loads");
        assert_eq!(state.tick.0, 240);
    }

    fn convert_tick_rate(mut envelope: Value) -> Result<Value, String> {
        envelope["tick_hz"] = TICK_HZ.into();
        Ok(envelope)
    }

    /// Spec 017 section 2: the tick-rate refusal holds until a
    /// migration explicitly converts it; with a converting step in
    /// the chain, the same save loads.
    #[test]
    fn migration_may_convert_tick_rate() {
        let mut envelope = current_envelope();
        envelope["tick_hz"] = 10.into();
        // Pin the walk to a version-1 claim so the synthetic chain
        // below is what runs, whatever the current version is. The
        // payload stays current-format; only the rate rule is under
        // test here.
        envelope["format_version"] = 1.into();
        let bytes = serde_json::to_vec(&envelope).unwrap();
        const CONVERTS: &[Migration] = &[Migration {
            from: 1,
            run: convert_tick_rate,
        }];
        match decode_with(&bytes, &[], 1) {
            Err(SaveError::TickRateMismatch { saved, expected }) => {
                assert_eq!(saved, 10);
                assert_eq!(expected, TICK_HZ);
            }
            Err(other) => panic!("wrong error: {other:?}"),
            Ok(_) => panic!("the mismatch loaded without a converting migration"),
        }
        decode_with(&bytes, CONVERTS, 2).expect("the converting migration lifts the refusal");
    }
}
