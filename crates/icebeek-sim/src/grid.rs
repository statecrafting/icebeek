//! The buildable interior grid (spec 015): a per-deck cell grid with
//! strictly finite hull space, room modules on rectangular cell
//! footprints, and the logistics spine as typed edges between
//! adjacent cells. Build, tear-out, and re-route are typed player
//! commands validated deterministically in the commands phase; an
//! invalid order drops with a typed rejection and touches nothing.
//! Breach state derives from hull stress through the stored
//! cell-to-node mapping: a breached cell disables its rooms and
//! severs its edges, so cascade reach follows spine topology.

use bevy_ecs::prelude::Resource;
use serde::{Deserialize, Serialize};

use crate::state::{Command, EngineCore, HULL_NODES, HullGraph, ShipSystem, ThermalField};

/// Deck count for this slice. Dimensions are balancing data; their
/// existence and finiteness are pinned (spec 015 section 2).
pub const DECKS: u8 = 1;
/// Cells across a deck (the hull axis; column 0 is the bow side).
pub const GRID_W: usize = 16;
/// Cells deep; row 0 is the hull-adjacent row where equipment lives
/// and breaches open.
pub const GRID_H: usize = 8;

/// Fraction of the build cost refunded on removal (spec 015 section
/// 5: refit is an expected activity). Balancing data; the refund's
/// existence is pinned. Applied as floor(cost / 2).
pub const REFUND_DIVISOR: u64 = 2;
/// Cell temperature below which a room stalls (spec 004 section 5:
/// cold stalls machinery; it kills only biology).
pub const FREEZE_STALL_C: f32 = 0.0;
/// Hull-node stress at which the node's hull-row cells are breached.
pub const BREACH_STRESS: f32 = 0.95;
/// Frozen scrap one spine edge costs to lay. Edge removal refunds
/// floor(1 / 2) = 0: declared, not accidental.
pub const EDGE_COST_SCRAP: u64 = 1;

/// A grid address (spec 015 section 2). The derived `Ord` is the
/// lexicographic (deck, x, y) order every state-affecting pass uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CellAddr {
    pub deck: u8,
    pub x: u8,
    pub y: u8,
}

impl CellAddr {
    pub fn in_bounds(&self) -> bool {
        self.deck < DECKS && (self.x as usize) < GRID_W && (self.y as usize) < GRID_H
    }

    /// Row-major index into per-cell state like the thermal field.
    pub fn index(&self) -> usize {
        self.y as usize * GRID_W + self.x as usize
    }

    /// Manhattan-adjacent on the same deck: what a spine edge may span.
    pub fn adjacent_to(&self, other: &CellAddr) -> bool {
        self.deck == other.deck && self.x.abs_diff(other.x) + self.y.abs_diff(other.y) == 1
    }
}

/// The room roster (spec 004 section 4, spec 015 section 3). The
/// engine core is fixed and pre-placed, never buildable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoomKind {
    EngineCore,
    Foundry,
    Refinery,
    Fabricator,
    Hydroponics,
    DroneBay,
    HeatSink,
    Storage,
    Strut,
}

/// One room type's data (spec 015 section 3). Values are balancing
/// placeholders; the axes (footprint, mass, heat, buffers, cost) are
/// the spec's.
#[derive(Debug, Clone, PartialEq)]
pub struct RoomSpec {
    /// Footprint in cells, (width, height).
    pub footprint: (u8, u8),
    /// Mass contributed to the spec 005 section 5 efficiency tax.
    pub mass: f32,
    /// Heat emitted (positive) or absorbed (negative) per second
    /// while working, spread over the footprint.
    pub heat_per_second: f32,
    /// Build cost drawn from cargo, indexed like `ResourceKind::ALL`.
    pub build_cost: [u64; 4],
    /// Capacity of each machine buffer (input and output).
    pub buffer_capacity: u32,
    /// The tech tier that unlocks placing this room (spec 016
    /// section 4).
    pub min_tier: u8,
}

const ENGINE_CORE: RoomSpec = RoomSpec {
    footprint: (2, 2),
    mass: 200.0,
    // The core heats its cells through the dedicated coupling term,
    // not a room emission.
    heat_per_second: 0.0,
    build_cost: [0, 0, 0, 0],
    buffer_capacity: 0,
    min_tier: 1,
};
const FOUNDRY: RoomSpec = RoomSpec {
    footprint: (2, 2),
    mass: 60.0,
    heat_per_second: 25.0,
    build_cost: [20, 0, 0, 0],
    buffer_capacity: 8,
    min_tier: 1,
};
const REFINERY: RoomSpec = RoomSpec {
    footprint: (2, 2),
    mass: 60.0,
    heat_per_second: 20.0,
    build_cost: [15, 5, 0, 0],
    buffer_capacity: 8,
    min_tier: 1,
};
const FABRICATOR: RoomSpec = RoomSpec {
    footprint: (2, 2),
    mass: 50.0,
    heat_per_second: 15.0,
    build_cost: [15, 0, 0, 0],
    buffer_capacity: 8,
    min_tier: 2,
};
const HYDROPONICS: RoomSpec = RoomSpec {
    footprint: (2, 2),
    mass: 40.0,
    heat_per_second: 5.0,
    build_cost: [10, 0, 10, 0],
    buffer_capacity: 8,
    min_tier: 2,
};
const DRONE_BAY: RoomSpec = RoomSpec {
    footprint: (2, 2),
    mass: 50.0,
    heat_per_second: 10.0,
    build_cost: [15, 0, 0, 0],
    buffer_capacity: 4,
    min_tier: 1,
};
const HEAT_SINK: RoomSpec = RoomSpec {
    footprint: (1, 1),
    mass: 20.0,
    heat_per_second: -30.0,
    build_cost: [8, 0, 0, 0],
    buffer_capacity: 0,
    min_tier: 1,
};
const STORAGE: RoomSpec = RoomSpec {
    footprint: (2, 2),
    mass: 30.0,
    heat_per_second: 0.0,
    build_cost: [10, 0, 0, 0],
    buffer_capacity: 12,
    min_tier: 1,
};
const STRUT: RoomSpec = RoomSpec {
    footprint: (1, 1),
    mass: 10.0,
    heat_per_second: 0.0,
    build_cost: [2, 0, 0, 0],
    buffer_capacity: 0,
    min_tier: 1,
};

/// The room table (spec 015 section 3), exhaustive by construction:
/// a kind with no entry is a compile error, never a runtime default.
pub const fn room_spec(kind: RoomKind) -> &'static RoomSpec {
    match kind {
        RoomKind::EngineCore => &ENGINE_CORE,
        RoomKind::Foundry => &FOUNDRY,
        RoomKind::Refinery => &REFINERY,
        RoomKind::Fabricator => &FABRICATOR,
        RoomKind::Hydroponics => &HYDROPONICS,
        RoomKind::DroneBay => &DRONE_BAY,
        RoomKind::HeatSink => &HEAT_SINK,
        RoomKind::Storage => &STORAGE,
        RoomKind::Strut => &STRUT,
    }
}

/// A placed room module (spec 015 section 3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Room {
    /// Assigned from the monotonic counter at placement; stable.
    pub id: u32,
    pub kind: RoomKind,
    /// Lowest-addressed cell of the footprint.
    pub origin: CellAddr,
    /// Machine buffers, in whole units (spec 015 section 3).
    pub input_buffer: u32,
    pub output_buffer: u32,
}

impl Room {
    pub fn spec(&self) -> &'static RoomSpec {
        room_spec(self.kind)
    }

    /// Footprint cells in lexicographic (deck, x, y) order.
    pub fn cells(&self) -> impl Iterator<Item = CellAddr> + '_ {
        let (w, h) = self.spec().footprint;
        (0..w).flat_map(move |dx| {
            (0..h).map(move |dy| CellAddr {
                deck: self.origin.deck,
                x: self.origin.x + dx,
                y: self.origin.y + dy,
            })
        })
    }

    pub fn contains(&self, cell: CellAddr) -> bool {
        let (w, h) = self.spec().footprint;
        cell.deck == self.origin.deck
            && cell.x >= self.origin.x
            && cell.x < self.origin.x + w
            && cell.y >= self.origin.y
            && cell.y < self.origin.y + h
    }
}

/// The three spine edge types (spec 004 section 4, spec 015 section
/// 4). The rule-language surface over data lines is a future spec
/// (spec 015 section 8); the edge type exists so layouts and refits
/// are real today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpineKind {
    Belt,
    Pipe,
    DataLine,
}

/// One typed edge over a cell boundary, with a fixed direction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpineEdge {
    pub id: u32,
    pub kind: SpineKind,
    pub from: CellAddr,
    pub to: CellAddr,
}

/// Why a build order was dropped (spec 015 section 5): typed,
/// readable by the UI, never a partial apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    OutOfBounds,
    Overlap,
    Unaffordable,
    /// The engine core is fixed: never placed, removed, or jettisoned.
    FixedRoom,
    UnknownRoom,
    UnknownEdge,
    NotAdjacent,
    DuplicateEdge,
    /// The order needs a higher tech tier (spec 016 section 4).
    TierGated,
    /// Tier advancement needs its blueprint set complete first (spec
    /// 016 section 3).
    IncompleteBlueprints,
}

/// A dropped order and its reason.
#[derive(Debug, Clone, PartialEq)]
pub struct BuildRejection {
    pub command: Command,
    pub reason: RejectReason,
}

/// The tick's typed rejections, readable by the UI (spec 015 section
/// 5). Diagnostic surface like the phase log: cleared every tick,
/// never part of the save.
#[derive(Debug, Clone, Default, Resource)]
pub struct BuildLog(pub Vec<BuildRejection>);

/// The interior grid domain (spec 010 section 5, spec 015). Vec
/// order is creation order for both rooms and edges, and creation
/// order is processing order (determinism discipline).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Resource)]
pub struct InteriorGrid {
    pub rooms: Vec<Room>,
    pub next_room_id: u32,
    pub edges: Vec<SpineEdge>,
    pub next_edge_id: u32,
    /// The fixed, deterministic cell-to-hull-node mapping (spec 015
    /// section 2): every column belongs to one hull node's band. It
    /// serializes with the save.
    pub hull_node_of_column: [u8; GRID_W],
}

impl Default for InteriorGrid {
    fn default() -> Self {
        let mut hull_node_of_column = [0u8; GRID_W];
        for (x, node) in hull_node_of_column.iter_mut().enumerate() {
            *node = (x * HULL_NODES / GRID_W) as u8;
        }
        Self {
            // The engine core is pre-placed amidships (spec 015
            // section 3), away from the hull row.
            rooms: vec![Room {
                id: 0,
                kind: RoomKind::EngineCore,
                origin: CellAddr {
                    deck: 0,
                    x: 7,
                    y: 3,
                },
                input_buffer: 0,
                output_buffer: 0,
            }],
            next_room_id: 1,
            edges: Vec::new(),
            next_edge_id: 0,
            hull_node_of_column,
        }
    }
}

impl InteriorGrid {
    /// Total mass of every placed room: the sprawl side of the spec
    /// 005 section 5 efficiency tax.
    pub fn room_mass(&self) -> f32 {
        self.rooms.iter().map(|room| room.spec().mass).sum()
    }

    pub fn room_index_at(&self, cell: CellAddr) -> Option<usize> {
        self.rooms.iter().position(|room| room.contains(cell))
    }

    /// The row-major cell index of the engine core's origin cell:
    /// where the core couples into the thermal field.
    pub fn core_cell_index(&self) -> usize {
        self.rooms
            .iter()
            .find(|room| room.kind == RoomKind::EngineCore)
            .map(|room| room.origin.index())
            .unwrap_or(0)
    }

    /// Breach state is derived, not stored (spec 015 section 4): a
    /// hull-row cell is breached while its column's hull node is at
    /// breach stress. Repairing below the threshold seals it.
    pub fn cell_breached(&self, cell: CellAddr, hull: &HullGraph) -> bool {
        cell.y == 0
            && hull.stress[self.hull_node_of_column[cell.x as usize] as usize] >= BREACH_STRESS
    }

    /// A room is disabled while any footprint cell is breached (spec
    /// 015 section 4: a breach disables its cell's rooms).
    pub fn room_disabled(&self, room: &Room, hull: &HullGraph) -> bool {
        room.cells().any(|cell| self.cell_breached(cell, hull))
    }

    /// The power-and-cold stall rule (spec 015 section 3): an
    /// unpowered or freezing room stops working before it breaks.
    /// The engine core is the power plant and exempts itself; breach
    /// disablement composes on top.
    pub fn room_working(
        &self,
        room: &Room,
        core: &EngineCore,
        field: &ThermalField,
        hull: &HullGraph,
    ) -> bool {
        if self.room_disabled(room, hull) {
            return false;
        }
        if room.kind == RoomKind::EngineCore {
            return true;
        }
        if !core.system_online(ShipSystem::Logistics) {
            return false;
        }
        room.cells()
            .all(|cell| field.temps[cell.index()] >= FREEZE_STALL_C)
    }

    /// An edge is severed while either endpoint cell is breached or
    /// belongs to a disabled room (spec 015 section 4).
    pub fn edge_severed(&self, edge: &SpineEdge, hull: &HullGraph) -> bool {
        for cell in [edge.from, edge.to] {
            if self.cell_breached(cell, hull) {
                return true;
            }
            if let Some(index) = self.room_index_at(cell)
                && self.room_disabled(&self.rooms[index], hull)
            {
                return true;
            }
        }
        false
    }

    /// Placement validation (spec 015 section 5): bounds, overlap.
    /// Cost checks live with the cargo write in the commands phase.
    pub fn placement_rejection(&self, kind: RoomKind, origin: CellAddr) -> Option<RejectReason> {
        if kind == RoomKind::EngineCore {
            return Some(RejectReason::FixedRoom);
        }
        let (w, h) = room_spec(kind).footprint;
        let far = CellAddr {
            deck: origin.deck,
            x: origin.x.saturating_add(w).saturating_sub(1),
            y: origin.y.saturating_add(h).saturating_sub(1),
        };
        if !origin.in_bounds() || !far.in_bounds() {
            return Some(RejectReason::OutOfBounds);
        }
        let probe = Room {
            id: u32::MAX,
            kind,
            origin,
            input_buffer: 0,
            output_buffer: 0,
        };
        if probe.cells().any(|cell| self.room_index_at(cell).is_some()) {
            return Some(RejectReason::Overlap);
        }
        None
    }

    /// Edge validation (spec 015 section 5).
    pub fn edge_rejection(
        &self,
        kind: SpineKind,
        from: CellAddr,
        to: CellAddr,
    ) -> Option<RejectReason> {
        if !from.in_bounds() || !to.in_bounds() {
            return Some(RejectReason::OutOfBounds);
        }
        if !from.adjacent_to(&to) {
            return Some(RejectReason::NotAdjacent);
        }
        if self
            .edges
            .iter()
            .any(|edge| edge.kind == kind && edge.from == from && edge.to == to)
        {
            return Some(RejectReason::DuplicateEdge);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stored cell-to-node mapping is deterministic and total:
    /// every column maps into the hull graph, in band order.
    #[test]
    fn column_mapping_is_total_and_monotonic() {
        let grid = InteriorGrid::default();
        let mut last = 0u8;
        for node in grid.hull_node_of_column {
            assert!((node as usize) < HULL_NODES);
            assert!(node >= last, "bands must not interleave");
            last = node;
        }
        assert_eq!(grid.hull_node_of_column[0], 0);
        assert_eq!(
            grid.hull_node_of_column[GRID_W - 1] as usize,
            HULL_NODES - 1
        );
    }

    /// The power-and-cold stall rule, directly (spec 015 section 3).
    #[test]
    fn rooms_stall_unpowered_or_freezing_before_breaking() {
        let grid = InteriorGrid::default();
        let hull = HullGraph::default();
        let mut field = ThermalField::default();
        let room = Room {
            id: 9,
            kind: RoomKind::Foundry,
            origin: CellAddr {
                deck: 0,
                x: 1,
                y: 4,
            },
            input_buffer: 0,
            output_buffer: 0,
        };
        let mut core = EngineCore::default();
        assert!(grid.room_working(&room, &core, &field, &hull));
        // Unpowered: the ladder has logistics down.
        core.shutdown_stage = 3;
        assert!(!grid.room_working(&room, &core, &field, &hull));
        core.shutdown_stage = 0;
        // Freezing: one footprint cell below the stall line.
        field.temps[room.origin.index()] = FREEZE_STALL_C - 5.0;
        assert!(!grid.room_working(&room, &core, &field, &hull));
    }
}
