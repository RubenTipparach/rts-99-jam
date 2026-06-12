//! Roster-expansion meshes: the new Astromancer casters, Hollowmen chassis
//! variants, and the tech tree buildings from the concept sheets
//! (`assets/concepts/roster_*.png`). Same conventions as the launch set in
//! `main.rs`: world units, +y up, ground at y = 0, units face +z (visor /
//! eye side) except static defenses which fire toward -z, building doors
//! face +z, and the team-tint channel rides each design's signature element.
//!
//! Like every model here, these are scaffolds: the checked-in OBJ/MTL files
//! are the source of truth and get refined in Blender.

use super::*;

// Shared faction palettes (match the launch models).
const SHELL: [f32; 3] = [0.86, 0.84, 0.76];
const SHELL2: [f32; 3] = [0.74, 0.72, 0.64];
const SHELL3: [f32; 3] = [0.64, 0.62, 0.55];
const GOLD: [f32; 3] = [0.86, 0.75, 0.45];
const GOLD_DK: [f32; 3] = [0.70, 0.58, 0.32];
const AETHER: [f32; 3] = [0.55, 0.92, 0.86];
const STEEL: [f32; 3] = [0.46, 0.49, 0.53];
const STEEL2: [f32; 3] = [0.58, 0.61, 0.65];
const STEEL3: [f32; 3] = [0.38, 0.41, 0.45];
const DARK: [f32; 3] = [0.24, 0.26, 0.30];
const HAZ: [f32; 3] = [0.80, 0.58, 0.20];
const GUN: [f32; 3] = [0.17, 0.19, 0.22];
const AMBER: [f32; 3] = [0.95, 0.75, 0.35];
const SENSOR: [f32; 3] = [0.44, 0.78, 0.88];
const CONCRETE: [f32; 3] = [0.52, 0.53, 0.51];
const CONCRETE2: [f32; 3] = [0.40, 0.41, 0.39];
const TEAM: [f32; 3] = [0.5, 0.5, 0.5];
// School accents.
const EMBER: [f32; 3] = [0.95, 0.45, 0.20];
const FLAME: [f32; 3] = [0.98, 0.76, 0.36];
const STORM: [f32; 3] = [0.45, 0.75, 0.95];
const HEXG: [f32; 3] = [0.55, 0.92, 0.45];
const MOSS: [f32; 3] = [0.55, 0.66, 0.38];
const BARK: [f32; 3] = [0.42, 0.32, 0.22];
const SAND: [f32; 3] = [0.98, 0.86, 0.55];
const VIOLET: [f32; 3] = [0.78, 0.55, 0.95];

// ------------------------------------------------------------ shared parts

/// The shared Athenaeum-caster body: a grown hovering robe (no legs, like
/// the Acolyte it rests just above y = 0 and the engine lifts it), with a
/// team-tinted chest stole and glowing eyes on the +z face. Heads and hats
/// differ per school, so the body stops at the shoulders (y about 1.75).
fn push_robe(m: &mut Vec<UnitVertex>, robe: [f32; 3], robe2: [f32; 3], eyes: [f32; 3]) {
    // Hem-to-skirt flare, then the torso and a carved shoulder mantle.
    push_frustum(
        m, 0.0, 0.0, 0.18, 0.44, 0.06, 0.16, robe2, 0.0, 6, 0.0, false,
    );
    push_frustum(
        m, 0.0, 0.0, 0.44, 0.30, 0.16, 0.95, robe, 0.0, 6, 0.0, false,
    );
    push_frustum(
        m, 0.0, 0.0, 0.30, 0.35, 0.95, 1.45, robe, 0.0, 6, 0.0, false,
    );
    push_frustum(
        m, 0.0, 0.0, 0.37, 0.20, 1.45, 1.75, robe2, 0.0, 6, 0.0, true,
    );
    // Team stole: a tinted band across the chest.
    push_box(m, [-0.30, 1.06, -0.30], [0.30, 1.24, 0.34], TEAM, 1.0);
    // Head capsule with the glowing eye band facing +z.
    push_frustum(
        m, 0.0, 0.0, 0.13, 0.16, 1.85, 2.05, robe, 0.0, 6, 0.0, false,
    );
    push_box(m, [-0.10, 1.90, 0.13], [0.10, 1.98, 0.19], eyes, 0.0);
}

/// A simple caster staff at `(x, z)`: a shaft with a gold collar, topped by
/// a school-colored gem.
fn push_staff(m: &mut Vec<UnitVertex>, x: f32, z: f32, top: f32, gem: [f32; 3]) {
    push_prism(m, x, z, 0.035, 0.06, top, BARK, 0.0, 6, 0.0, false);
    push_prism(m, x, z, 0.055, top - 0.10, top, GOLD_DK, 0.0, 6, 0.0, false);
    push_gem(m, x, z, 0.10, top, top + 0.16, top + 0.36, gem, 0.0, 6);
}

/// A digitigrade mech leg pair: thigh, shin, and a toe-capped foot per side,
/// each leg kept on its own side of x = 0 so the baked walk cycle swings
/// them in counter-phase.
fn push_mech_legs(m: &mut Vec<UnitVertex>, spread: f32, hip: f32, w: f32) {
    for sx in [-1.0_f32, 1.0] {
        let x = sx * spread;
        push_box(
            m,
            [x - w, hip * 0.45, -0.20],
            [x + w, hip, 0.16],
            STEEL3,
            0.0,
        );
        push_box(
            m,
            [x - w * 0.8, 0.10, -0.12],
            [x + w * 0.8, hip * 0.55, 0.18],
            DARK,
            0.0,
        );
        push_box(m, [x - w, 0.0, -0.18], [x + w, 0.12, 0.30], GUN, 0.0);
        push_box(
            m,
            [x - w * 0.8, 0.0, 0.28],
            [x + w * 0.8, 0.09, 0.38],
            STEEL2,
            0.0,
        );
    }
}

/// Tracked running gear: a dark track block with a lighter guard plate per
/// side, hull left to the caller.
fn push_tracks(m: &mut Vec<UnitVertex>, hw: f32, hl: f32, h: f32) {
    for sx in [-1.0_f32, 1.0] {
        let (x0, x1) = (sx * hw, sx * (hw + 0.34));
        push_box(m, [x0.min(x1), 0.0, -hl], [x0.max(x1), h, hl], GUN, 0.0);
        push_box(
            m,
            [x0.min(x1) - 0.03, h, -hl - 0.03],
            [x0.max(x1) + 0.03, h + 0.10, hl + 0.03],
            STEEL3,
            0.0,
        );
    }
}

// ------------------------------------------------------- Astromancer units

/// Pyromancer: evocation caster. Horned hood, a brazier staff burning with a
/// flame gem, and an ember orb hovering at the off hand. ~2.9 tall, faces +z.
pub fn pyromancer_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_robe(&mut m, SHELL, SHELL2, FLAME);
    // Hood peak with two gold horns.
    push_pyramid(&mut m, 0.0, 0.0, 0.17, 2.02, 2.30, SHELL2, 0.0, 6, 0.0);
    for sx in [-1.0_f32, 1.0] {
        push_pyramid(
            &mut m,
            sx * 0.20,
            -0.02,
            0.06,
            2.06,
            2.36,
            GOLD,
            0.0,
            4,
            0.0,
        );
    }
    // Brazier staff: shaft, gold bowl, and the flame as a bright gem stack.
    push_prism(
        &mut m, 0.42, 0.16, 0.04, 0.06, 2.18, BARK, 0.0, 6, 0.0, false,
    );
    push_frustum(
        &mut m, 0.42, 0.16, 0.08, 0.17, 2.18, 2.34, GOLD, 0.0, 6, 0.0, false,
    );
    push_gem(&mut m, 0.42, 0.16, 0.13, 2.32, 2.52, 2.92, EMBER, 0.0, 6);
    push_gem(&mut m, 0.42, 0.16, 0.07, 2.42, 2.62, 2.86, FLAME, 0.0, 6);
    // The ember orb at the off hand.
    push_gem(&mut m, -0.46, 0.22, 0.11, 1.10, 1.24, 1.38, EMBER, 0.0, 6);
    m
}

/// Stormcaller: tempest caster, the anti-air answer. A tall conical hat with
/// a storm band, and a staff crowned by a charged disc with bolt shards
/// orbiting it. ~3.1 tall, faces +z.
pub fn stormcaller_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_robe(&mut m, SHELL, SHELL2, STORM);
    // Conical hat with a brim and a storm-blue band.
    push_frustum(
        &mut m, 0.0, 0.0, 0.30, 0.26, 2.02, 2.10, SHELL3, 0.0, 6, 0.0, false,
    );
    push_pyramid(&mut m, 0.0, 0.0, 0.24, 2.10, 2.78, SHELL2, 0.0, 6, 0.0);
    push_prism(
        &mut m, 0.0, 0.0, 0.205, 2.16, 2.26, STORM, 0.0, 6, 0.0, false,
    );
    // Staff with the charged storm disc and three orbiting bolt shards.
    push_prism(
        &mut m, 0.42, 0.16, 0.04, 0.06, 2.30, BARK, 0.0, 6, 0.0, false,
    );
    push_prism(
        &mut m, 0.42, 0.16, 0.20, 2.30, 2.40, STORM, 0.0, 8, 0.0, true,
    );
    for (k, &(dx, dz)) in [(0.28_f32, 0.0_f32), (-0.18, 0.24), (-0.18, -0.24)]
        .iter()
        .enumerate()
    {
        let y = 2.52 + 0.12 * k as f32;
        push_gem(
            &mut m,
            0.42 + dx,
            0.16 + dz,
            0.045,
            y,
            y + 0.10,
            y + 0.22,
            STORM,
            0.0,
            4,
        );
    }
    m
}

/// Hex-Witch: hexcraft debuffer. Wide-brim hat with a crooked two-step cone,
/// a darker violet robe, and three hex sigils orbiting the waist. ~2.9 tall,
/// faces +z.
pub fn hex_witch_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let robe = [0.45, 0.34, 0.58];
    let robe2 = [0.36, 0.27, 0.48];
    push_robe(&mut m, robe, robe2, HEXG);
    // Brim, base cone, and the crooked tip leaning off-axis.
    push_prism(
        &mut m, 0.0, 0.0, 0.42, 2.02, 2.08, robe2, 0.0, 7, 0.0, false,
    );
    push_frustum(
        &mut m, 0.0, 0.0, 0.24, 0.12, 2.08, 2.42, robe2, 0.0, 6, 0.0, false,
    );
    push_frustum(
        &mut m, 0.08, 0.0, 0.12, 0.02, 2.42, 2.74, robe, 0.0, 6, 0.0, true,
    );
    // Crooked staff: shaft with an elbowed head and a hex gem.
    push_prism(
        &mut m, -0.42, 0.16, 0.035, 0.06, 1.95, BARK, 0.0, 6, 0.0, false,
    );
    push_box(&mut m, [-0.52, 1.95, 0.10], [-0.34, 2.05, 0.24], BARK, 0.0);
    push_gem(&mut m, -0.42, 0.16, 0.08, 2.10, 2.22, 2.38, HEXG, 0.0, 4);
    // Orbiting curse sigils at uneven heights.
    push_gem(&mut m, 0.52, 0.30, 0.07, 0.72, 0.82, 0.94, HEXG, 0.0, 4);
    push_gem(&mut m, -0.50, -0.34, 0.07, 1.30, 1.40, 1.52, VIOLET, 0.0, 4);
    push_gem(&mut m, 0.40, -0.44, 0.07, 1.66, 1.76, 1.88, HEXG, 0.0, 4);
    m
}

/// Druid: verdancy caster, the army's sustain. Antlered hood, a moss robe
/// with a bark girdle, and a living staff crowned by a leaf crystal. ~2.8
/// tall, faces +z.
pub fn druid_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let moss2 = [0.44, 0.54, 0.31];
    push_robe(&mut m, MOSS, moss2, [0.75, 0.98, 0.55]);
    push_pyramid(&mut m, 0.0, 0.0, 0.17, 2.02, 2.28, moss2, 0.0, 6, 0.0);
    // Bark girdle under the team stole.
    push_prism(
        &mut m, 0.0, 0.0, 0.315, 0.92, 1.04, BARK, 0.0, 6, 0.0, false,
    );
    // Antlers: a riser and two prongs per side.
    for sx in [-1.0_f32, 1.0] {
        let x = sx * 0.20;
        push_box(
            &mut m,
            [x - 0.03, 2.10, -0.03],
            [x + 0.03, 2.52, 0.03],
            BARK,
            0.0,
        );
        push_box(
            &mut m,
            [x - 0.03, 2.34, -0.03],
            [x + sx * 0.16, 2.40, 0.03],
            BARK,
            0.0,
        );
        push_box(
            &mut m,
            [x + sx * 0.13 - 0.03, 2.40, -0.03],
            [x + sx * 0.13 + 0.03, 2.66, 0.03],
            BARK,
            0.0,
        );
    }
    // Living staff with a leaf crystal, plus two seed gems drifting behind.
    push_staff(&mut m, 0.42, 0.16, 2.20, [0.62, 0.92, 0.45]);
    push_gem(&mut m, -0.40, -0.34, 0.06, 1.05, 1.13, 1.23, HEXG, 0.0, 4);
    push_gem(&mut m, 0.34, -0.42, 0.06, 1.45, 1.53, 1.63, HEXG, 0.0, 4);
    m
}

/// Evoker: summoner. A raised focus orb over the hood, a gold summoning
/// ring floating at the waist, and two aether construct shards condensing
/// at the shoulders. ~2.9 tall, faces +z.
pub fn evoker_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_robe(&mut m, SHELL, SHELL2, AETHER);
    push_pyramid(&mut m, 0.0, 0.0, 0.17, 2.02, 2.30, SHELL2, 0.0, 6, 0.0);
    // The focus orb hovering above the hood peak.
    push_gem(&mut m, 0.0, 0.0, 0.13, 2.42, 2.58, 2.78, VIOLET, 0.0, 6);
    // Gold summoning ring around the waist.
    push_prism(
        &mut m, 0.0, 0.0, 0.52, 1.00, 1.06, GOLD, 0.0, 10, 0.0, false,
    );
    // Construct shards taking shape beside each shoulder.
    for sx in [-1.0_f32, 1.0] {
        let x = sx * 0.55;
        push_gem(&mut m, x, 0.10, 0.10, 1.50, 1.66, 1.86, AETHER, 0.0, 6);
        push_gem(&mut m, x, 0.10, 0.05, 1.92, 1.98, 2.06, AETHER, 0.0, 4);
    }
    m
}

/// Chronomancer: the tier-3 time elite. A taller crowned robe, a floating
/// hourglass at the chest, and a ring of gold hour-marks orbiting the waist.
/// ~3.1 tall, faces +z.
pub fn chronomancer_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_robe(&mut m, SHELL, SHELL2, SAND);
    // Crown: a gold circlet with four points over the hood peak.
    push_pyramid(&mut m, 0.0, 0.0, 0.17, 2.02, 2.34, SHELL2, 0.0, 6, 0.0);
    push_prism(
        &mut m, 0.0, 0.0, 0.155, 2.10, 2.18, GOLD, 0.0, 6, 0.0, false,
    );
    for k in 0..4 {
        let a = std::f32::consts::TAU * k as f32 / 4.0;
        push_pyramid(
            &mut m,
            a.cos() * 0.14,
            a.sin() * 0.14,
            0.035,
            2.18,
            2.40,
            GOLD,
            0.0,
            4,
            a,
        );
    }
    // The hourglass floating ahead of the chest: gold caps, sand bulbs.
    let (hz, hy) = (0.52_f32, 1.18_f32);
    push_prism(
        &mut m,
        0.0,
        hz,
        0.13,
        hy - 0.04,
        hy,
        GOLD,
        0.0,
        6,
        0.0,
        true,
    );
    push_frustum(
        &mut m,
        0.0,
        hz,
        0.10,
        0.03,
        hy,
        hy + 0.14,
        SAND,
        0.0,
        6,
        0.0,
        false,
    );
    push_frustum(
        &mut m,
        0.0,
        hz,
        0.03,
        0.10,
        hy + 0.14,
        hy + 0.28,
        SAND,
        0.0,
        6,
        0.0,
        false,
    );
    push_prism(
        &mut m,
        0.0,
        hz,
        0.13,
        hy + 0.28,
        hy + 0.32,
        GOLD,
        0.0,
        6,
        0.0,
        true,
    );
    // The hour-marks: eight small gold gems ringing the waist.
    for k in 0..8 {
        let a = std::f32::consts::TAU * k as f32 / 8.0;
        push_gem(
            &mut m,
            a.cos() * 0.62,
            a.sin() * 0.62,
            0.04,
            1.06,
            1.11,
            1.17,
            GOLD,
            0.0,
            4,
        );
    }
    m
}

/// Seer: the detector familiar, a floating carved eye. Team tint rides the
/// iris; a gold brow ring and lash spikes crown it, scry-shards trail below.
/// Rests low like the Acolyte; the engine lifts it into its hover. Faces +z.
pub fn seer_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    // The eye body: two frustums meeting at the girdle.
    push_frustum(
        &mut m, 0.0, 0.0, 0.22, 0.42, 0.30, 0.62, SHELL, 0.0, 8, 0.0, false,
    );
    push_frustum(
        &mut m, 0.0, 0.0, 0.42, 0.22, 0.62, 0.94, SHELL2, 0.0, 8, 0.0, true,
    );
    // Iris and pupil on the +z face: the iris is the team channel.
    push_box(&mut m, [-0.17, 0.44, 0.36], [0.17, 0.74, 0.44], TEAM, 1.0);
    push_box(
        &mut m,
        [-0.07, 0.52, 0.42],
        [0.07, 0.68, 0.48],
        [0.20, 0.16, 0.30],
        0.0,
    );
    // Gold brow ring and four lash spikes.
    push_prism(&mut m, 0.0, 0.0, 0.27, 0.94, 0.99, GOLD, 0.0, 8, 0.0, false);
    for k in 0..4 {
        let a = std::f32::consts::TAU * (k as f32 + 0.5) / 4.0;
        push_pyramid(
            &mut m,
            a.cos() * 0.20,
            a.sin() * 0.20,
            0.045,
            0.99,
            1.22,
            GOLD,
            0.0,
            4,
            a,
        );
    }
    // Scry-shards trailing beneath.
    push_gem(&mut m, 0.16, -0.10, 0.05, 0.10, 0.16, 0.24, AETHER, 0.0, 4);
    push_gem(&mut m, -0.20, 0.06, 0.04, 0.16, 0.21, 0.28, AETHER, 0.0, 4);
    m
}

/// Wisp: the cheap flying scout, a darting mote. A small aether shard in a
/// carved halo with a team-tinted tip and a two-shard tail. Rests low; the
/// engine lifts it. Faces +z.
pub fn wisp_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_gem(&mut m, 0.0, 0.0, 0.14, 0.18, 0.46, 0.74, AETHER, 0.0, 5);
    push_pyramid(&mut m, 0.0, 0.0, 0.07, 0.74, 0.95, TEAM, 1.0, 5, 0.0);
    push_prism(
        &mut m, 0.0, 0.0, 0.24, 0.40, 0.45, SHELL2, 0.0, 8, 0.0, false,
    );
    push_gem(&mut m, 0.0, -0.34, 0.05, 0.34, 0.40, 0.48, AETHER, 0.0, 4);
    push_gem(&mut m, 0.0, -0.52, 0.035, 0.30, 0.34, 0.40, AETHER, 0.0, 4);
    m
}

/// Tempest Dais: the flying caster platform, a stone disc crewed by witches
/// riding its own storm. Hovers in-model like the Ward (clear gap below);
/// team tint bands the disc rim. ~4 wide, faces +z.
pub fn tempest_dais_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    // Underside keel and the deck disc, hovering from y = 0.9.
    push_frustum(
        &mut m, 0.0, 0.0, 0.5, 1.5, 0.9, 1.35, SHELL3, 0.0, 8, 0.0, false,
    );
    push_prism(&mut m, 0.0, 0.0, 1.7, 1.35, 1.62, SHELL, 0.0, 8, 0.0, true);
    push_prism(&mut m, 0.0, 0.0, 1.74, 1.40, 1.52, TEAM, 1.0, 8, 0.0, false);
    // Rail studs around the deck edge.
    for k in 0..8 {
        let a = std::f32::consts::TAU * (k as f32 + 0.5) / 8.0;
        let (px, pz) = (a.cos() * 1.5, a.sin() * 1.5);
        push_box(
            &mut m,
            [px - 0.07, 1.62, pz - 0.07],
            [px + 0.07, 1.92, pz + 0.07],
            GOLD_DK,
            0.0,
        );
    }
    // The central storm focus: a charged crystal on a carved plinth.
    push_frustum(
        &mut m, 0.0, 0.0, 0.30, 0.18, 1.62, 1.95, SHELL2, 0.0, 6, 0.0, false,
    );
    push_gem(&mut m, 0.0, 0.0, 0.22, 1.95, 2.30, 2.75, STORM, 0.0, 6);
    // Two crew witches at the rail (tiny robed figures with cone hats).
    for (cx, cz) in [(-0.85_f32, 0.35_f32), (0.75, -0.5)] {
        push_frustum(
            &mut m,
            cx,
            cz,
            0.20,
            0.13,
            1.62,
            2.10,
            [0.45, 0.34, 0.58],
            0.0,
            6,
            0.0,
            false,
        );
        push_prism(
            &mut m,
            cx,
            cz,
            0.22,
            2.10,
            2.15,
            [0.36, 0.27, 0.48],
            0.0,
            6,
            0.0,
            false,
        );
        push_pyramid(
            &mut m,
            cx,
            cz,
            0.13,
            2.15,
            2.45,
            [0.36, 0.27, 0.48],
            0.0,
            6,
            0.0,
        );
    }
    // Storm shards crackling under the keel.
    push_gem(&mut m, 0.55, 0.30, 0.07, 0.42, 0.52, 0.64, STORM, 0.0, 4);
    push_gem(&mut m, -0.50, -0.25, 0.06, 0.55, 0.63, 0.73, STORM, 0.0, 4);
    m
}

// --------------------------------------------------------- Hollowmen units

/// Hound: the light recon mech, a walking radar set. Digitigrade legs, a
/// compact hull with a team visor, the spotter dish, and a whip antenna.
/// ~2.4 tall, faces +z.
pub fn hound_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_mech_legs(&mut m, 0.30, 0.85, 0.11);
    // Hull with the team-tinted visor strip and a hazard flank band.
    push_box(&mut m, [-0.44, 0.85, -0.42], [0.44, 1.28, 0.40], STEEL, 0.0);
    push_box(&mut m, [-0.30, 0.96, 0.38], [0.30, 1.14, 0.46], TEAM, 1.0);
    push_box(&mut m, [-0.46, 1.00, -0.44], [0.46, 1.12, -0.40], HAZ, 0.0);
    // The radar dish on a yoke, opening skyward, with a sensor feed.
    push_box(
        &mut m,
        [-0.09, 1.28, -0.20],
        [0.09, 1.46, -0.02],
        STEEL3,
        0.0,
    );
    push_frustum(
        &mut m, 0.0, -0.11, 0.12, 0.45, 1.46, 1.70, STEEL2, 0.0, 8, 0.0, false,
    );
    push_prism(
        &mut m, 0.0, -0.11, 0.07, 1.46, 1.78, SENSOR, 0.0, 6, 0.0, true,
    );
    // Whip antenna with a tip light.
    push_prism(
        &mut m, 0.34, -0.30, 0.02, 1.28, 2.30, GUN, 0.0, 6, 0.0, false,
    );
    push_box(&mut m, [0.31, 2.30, -0.33], [0.37, 2.36, -0.27], AMBER, 0.0);
    m
}

/// Javelin: the missile mech, ranged fire support against ground and air.
/// Heavier legs, a plated torso with a team sensor head, and two shoulder
/// pods with loaded missile cells facing +z. ~2.7 tall.
pub fn javelin_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_mech_legs(&mut m, 0.36, 1.0, 0.14);
    push_box(&mut m, [-0.36, 1.0, -0.26], [0.36, 1.2, 0.26], DARK, 0.0); // waist
    push_box(&mut m, [-0.50, 1.2, -0.36], [0.50, 1.95, 0.34], STEEL, 0.0);
    push_box(&mut m, [-0.50, 1.36, 0.34], [0.50, 1.50, 0.38], HAZ, 0.0);
    // Sensor head with the team-tinted targeting visor.
    push_box(
        &mut m,
        [-0.16, 1.95, -0.12],
        [0.16, 2.20, 0.18],
        STEEL2,
        0.0,
    );
    push_box(&mut m, [-0.11, 2.02, 0.18], [0.11, 2.13, 0.24], TEAM, 1.0);
    // Shoulder pods: armored boxes with a 3x2 grid of missile cells.
    for sx in [-1.0_f32, 1.0] {
        let (x0, x1) = (sx * 0.52, sx * 0.96);
        push_box(
            &mut m,
            [x0.min(x1), 1.55, -0.38],
            [x0.max(x1), 2.10, 0.34],
            GUN,
            0.0,
        );
        push_box(
            &mut m,
            [x0.min(x1), 2.10, -0.30],
            [x0.max(x1), 2.18, 0.26],
            STEEL3,
            0.0,
        );
        for r in 0..2 {
            for c in 0..3 {
                let cx = x0.min(x1) + 0.07 + c as f32 * 0.13;
                let cy = 1.66 + r as f32 * 0.24;
                push_box(
                    &mut m,
                    [cx, cy, 0.34],
                    [cx + 0.09, cy + 0.14, 0.40],
                    AMBER,
                    0.0,
                );
            }
        }
    }
    // Comms mast off the right pod.
    push_prism(
        &mut m, 0.88, -0.28, 0.02, 2.18, 2.65, GUN, 0.0, 6, 0.0, false,
    );
    m
}

/// Wrecker: the melee mech, a caster-diving line-breaker. Hunched hull
/// thrown forward over massive clawed forearms, the thermal lance glowing
/// between the right claws, team plate on the cowl. ~2.5 tall, faces +z.
pub fn wrecker_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_mech_legs(&mut m, 0.36, 0.95, 0.14);
    // Hunched hull: a lower block and an upper cowl thrown toward +z.
    push_box(&mut m, [-0.44, 0.95, -0.38], [0.44, 1.36, 0.22], STEEL, 0.0);
    push_box(
        &mut m,
        [-0.50, 1.36, -0.20],
        [0.50, 1.80, 0.52],
        STEEL2,
        0.0,
    );
    push_box(&mut m, [-0.34, 1.80, -0.12], [0.34, 1.92, 0.44], TEAM, 1.0);
    push_box(&mut m, [-0.50, 1.48, 0.52], [0.50, 1.60, 0.56], HAZ, 0.0);
    // Low-set head glaring out from under the cowl.
    push_box(&mut m, [-0.14, 1.40, 0.52], [0.14, 1.60, 0.68], DARK, 0.0);
    push_box(
        &mut m,
        [-0.10, 1.45, 0.68],
        [0.10, 1.55, 0.72],
        [0.92, 0.30, 0.22],
        0.0,
    );
    // Breaching forearms: shoulder, the dropped forearm, three claws each.
    for sx in [-1.0_f32, 1.0] {
        let (x0, x1) = (sx * 0.50, sx * 0.82);
        push_box(
            &mut m,
            [x0.min(x1), 1.30, -0.12],
            [x0.max(x1), 1.70, 0.26],
            STEEL3,
            0.0,
        );
        push_box(
            &mut m,
            [x0.min(x1) + 0.02, 0.42, 0.06],
            [x0.max(x1) - 0.02, 1.36, 0.52],
            DARK,
            0.0,
        );
        for t in 0..3 {
            let cx = x0.min(x1) + 0.06 + t as f32 * 0.10;
            push_pyramid(
                &mut m,
                cx + 0.04,
                0.56,
                0.05,
                0.46,
                0.10,
                STEEL2,
                0.0,
                4,
                0.0,
            );
        }
    }
    // The thermal lance between the right claws.
    push_box(&mut m, [0.58, 0.50, 0.52], [0.74, 0.62, 0.86], AMBER, 0.0);
    m
}

/// Bulwark: the defense mech, a walking wall. Wide-set legs, a squat hull
/// with a team vision block, carrier arms holding the barrier projector
/// frame, and point-defense stubs on the shoulders. The wall of light spans
/// x = 0, so this chassis ships without baked walk frames (pose it in
/// Blender instead). ~2.5 tall, faces +z.
pub fn bulwark_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let barrier = [0.65, 0.88, 0.95];
    push_mech_legs(&mut m, 0.44, 0.95, 0.17);
    push_box(&mut m, [-0.66, 0.95, -0.40], [0.66, 1.60, 0.28], STEEL, 0.0);
    push_box(&mut m, [-0.66, 1.06, -0.42], [0.66, 1.20, -0.38], HAZ, 0.0);
    // Vision block: the team channel.
    push_box(
        &mut m,
        [-0.18, 1.60, -0.16],
        [0.18, 1.78, 0.10],
        STEEL2,
        0.0,
    );
    push_box(&mut m, [-0.13, 1.64, 0.10], [0.13, 1.74, 0.16], TEAM, 1.0);
    // Carrier arms out to the barrier frame.
    for sx in [-1.0_f32, 1.0] {
        push_box(
            &mut m,
            [sx * 0.5 - 0.10, 1.18, 0.28],
            [sx * 0.5 + 0.10, 1.36, 0.66],
            DARK,
            0.0,
        );
    }
    // The barrier: a framed wall of light (flat shaded, like aether).
    let wall_start = m.len();
    push_box(&mut m, [-1.0, 0.20, 0.66], [1.0, 1.75, 0.74], barrier, 0.0);
    set_detail(&mut m[wall_start..], DETAIL_FLAT);
    push_box(&mut m, [-1.06, 1.75, 0.62], [1.06, 1.86, 0.78], GUN, 0.0);
    push_box(&mut m, [-1.06, 0.10, 0.62], [1.06, 0.20, 0.78], GUN, 0.0);
    // Point-defense stubs.
    for sx in [-1.0_f32, 1.0] {
        push_box(
            &mut m,
            [sx * 0.44 - 0.09, 1.60, -0.30],
            [sx * 0.44 + 0.09, 1.76, -0.12],
            GUN,
            0.0,
        );
        push_prism(
            &mut m,
            sx * 0.44,
            -0.21,
            0.025,
            1.76,
            2.02,
            GUN,
            0.0,
            6,
            0.0,
            true,
        );
    }
    m
}

/// Earthshaker: the siege artillery tank. Tracked hull, a rear casemate
/// with a team band, the long gun stepped up to its firing angle toward
/// +z, and recoil spades at the rear. ~2.4 long per side of the origin.
pub fn earthshaker_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_tracks(&mut m, 0.54, 1.05, 0.42);
    push_box(&mut m, [-0.54, 0.30, -1.05], [0.54, 0.72, 1.05], STEEL, 0.0);
    push_box(&mut m, [-0.54, 0.44, 1.05], [0.54, 0.58, 1.09], HAZ, 0.0);
    // Rear casemate with the team identification band and a hatch.
    push_box(
        &mut m,
        [-0.40, 0.72, -0.95],
        [0.40, 1.18, -0.10],
        STEEL2,
        0.0,
    );
    push_box(&mut m, [-0.42, 0.92, -0.97], [0.42, 1.06, -0.08], TEAM, 1.0);
    push_box(&mut m, [-0.16, 1.18, -0.78], [0.16, 1.28, -0.44], DARK, 0.0);
    // The long gun, stepped up to its angle, with a muzzle brake.
    for k in 0..7 {
        let s = 0.115 - 0.007 * k as f32;
        let y0 = 0.88 + 0.165 * k as f32;
        let z0 = -0.25 + 0.29 * k as f32;
        push_box(&mut m, [-s, y0, z0], [s, y0 + 0.20, z0 + 0.40], GUN, 0.0);
    }
    push_box(&mut m, [-0.10, 2.02, 1.78], [0.10, 2.26, 1.98], STEEL3, 0.0);
    // Recoil spades biting the ground at the rear corners.
    for sx in [-1.0_f32, 1.0] {
        push_pyramid(
            &mut m,
            sx * 0.40,
            -1.18,
            0.13,
            0.62,
            0.0,
            STEEL2,
            0.0,
            4,
            0.0,
        );
    }
    m
}

/// Hailstorm: the flak tank, dedicated anti-air. Tracked hull, a turret
/// with a team band, four thin flak barrels stepped skyward toward +z, and
/// a tracking dish on the rear deck.
pub fn hailstorm_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_tracks(&mut m, 0.50, 0.88, 0.40);
    push_box(&mut m, [-0.50, 0.30, -0.88], [0.50, 0.68, 0.88], STEEL, 0.0);
    push_box(&mut m, [-0.50, 0.40, 0.88], [0.50, 0.52, 0.92], HAZ, 0.0);
    // Turret with the team band.
    push_box(
        &mut m,
        [-0.38, 0.68, -0.45],
        [0.38, 1.06, 0.32],
        STEEL2,
        0.0,
    );
    push_box(&mut m, [-0.40, 0.84, -0.47], [0.40, 0.96, 0.34], TEAM, 1.0);
    // Quad flak barrels, stepped up and forward.
    for sx in [-0.26_f32, -0.09, 0.09, 0.26] {
        for k in 0..3 {
            let y0 = 1.00 + 0.20 * k as f32;
            let z0 = 0.12 + 0.18 * k as f32;
            push_box(
                &mut m,
                [sx - 0.035, y0, z0],
                [sx + 0.035, y0 + 0.18, z0 + 0.26],
                GUN,
                0.0,
            );
        }
    }
    // Tracking dish on the rear deck.
    push_frustum(
        &mut m, 0.0, -0.62, 0.07, 0.26, 1.06, 1.22, STEEL2, 0.0, 8, 0.0, false,
    );
    push_prism(
        &mut m, 0.0, -0.62, 0.05, 1.06, 1.28, SENSOR, 0.0, 6, 0.0, true,
    );
    m
}

/// Interceptor: the air-superiority fighter. Authored resting on skids just
/// above the ground (the engine lifts flyers); nose toward +z, swept wings,
/// twin tail, a team-tinted canopy, and an afterburner ring at the back.
pub fn interceptor_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let alt = 0.35;
    // Fuselage with a two-step nose taper.
    push_box(
        &mut m,
        [-0.20, alt, -1.10],
        [0.20, alt + 0.38, 0.90],
        STEEL,
        0.0,
    );
    push_box(
        &mut m,
        [-0.14, alt + 0.04, 0.90],
        [0.14, alt + 0.32, 1.38],
        STEEL2,
        0.0,
    );
    push_box(
        &mut m,
        [-0.08, alt + 0.08, 1.38],
        [0.08, alt + 0.26, 1.70],
        STEEL2,
        0.0,
    );
    // Team-tinted canopy.
    push_box(
        &mut m,
        [-0.11, alt + 0.38, 0.10],
        [0.11, alt + 0.54, 0.55],
        TEAM,
        1.0,
    );
    // Swept wings: an inner panel and a swept-back outer panel per side.
    for sx in [-1.0_f32, 1.0] {
        let (x0, x1) = (sx * 0.20, sx * 0.82);
        push_box(
            &mut m,
            [x0.min(x1), alt + 0.12, -0.48],
            [x0.max(x1), alt + 0.21, 0.14],
            STEEL,
            0.0,
        );
        let (x2, x3) = (sx * 0.82, sx * 1.30);
        push_box(
            &mut m,
            [x2.min(x3), alt + 0.12, -0.74],
            [x2.max(x3), alt + 0.21, -0.26],
            STEEL2,
            0.0,
        );
    }
    // Tail fin and planes.
    push_box(
        &mut m,
        [-0.035, alt + 0.38, -1.08],
        [0.035, alt + 0.85, -0.72],
        STEEL2,
        0.0,
    );
    for sx in [-1.0_f32, 1.0] {
        let (x0, x1) = (sx * 0.18, sx * 0.52);
        push_box(
            &mut m,
            [x0.min(x1), alt + 0.30, -1.06],
            [x0.max(x1), alt + 0.37, -0.76],
            STEEL2,
            0.0,
        );
    }
    // Afterburner nozzle.
    push_tube_z(
        &mut m,
        0.0,
        alt + 0.19,
        0.11,
        -1.26,
        -1.10,
        GUN,
        0.0,
        8,
        false,
    );
    push_box(
        &mut m,
        [-0.08, alt + 0.12, -1.24],
        [0.08, alt + 0.26, -1.16],
        AMBER,
        0.0,
    );
    // Landing skids.
    for sx in [-1.0_f32, 1.0] {
        push_box(
            &mut m,
            [sx * 0.26 - 0.04, 0.0, -0.5],
            [sx * 0.26 + 0.04, alt + 0.02, 0.3],
            DARK,
            0.0,
        );
    }
    m
}

/// Vulture: the heavy bomber. Resting on skids; nose toward +z, broad
/// straight wings with twin engine nacelles each, a team-striped tail fin,
/// and the bomb bay doors under the belly.
pub fn vulture_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let alt = 0.42;
    // Heavy fuselage with a glazed nose step.
    push_box(
        &mut m,
        [-0.32, alt, -1.45],
        [0.32, alt + 0.55, 1.20],
        STEEL,
        0.0,
    );
    push_box(
        &mut m,
        [-0.24, alt + 0.05, 1.20],
        [0.24, alt + 0.46, 1.60],
        STEEL2,
        0.0,
    );
    push_box(
        &mut m,
        [-0.15, alt + 0.30, 1.42],
        [0.15, alt + 0.44, 1.58],
        SENSOR,
        0.0,
    );
    push_box(
        &mut m,
        [-0.32, alt + 0.22, -0.20],
        [0.32, alt + 0.32, -0.16],
        HAZ,
        0.0,
    );
    // Broad wings with two nacelles per side.
    for sx in [-1.0_f32, 1.0] {
        let (x0, x1) = (sx * 0.32, sx * 1.70);
        push_box(
            &mut m,
            [x0.min(x1), alt + 0.30, -0.32],
            [x0.max(x1), alt + 0.41, 0.40],
            STEEL,
            0.0,
        );
        for nx in [0.72_f32, 1.30] {
            let cx = sx * nx;
            push_box(
                &mut m,
                [cx - 0.09, alt + 0.10, -0.28],
                [cx + 0.09, alt + 0.30, 0.46],
                DARK,
                0.0,
            );
            push_box(
                &mut m,
                [cx - 0.06, alt + 0.14, -0.38],
                [cx + 0.06, alt + 0.26, -0.28],
                AMBER,
                0.0,
            );
        }
    }
    // Tail: the fin carries the team stripe.
    push_box(
        &mut m,
        [-0.045, alt + 0.55, -1.42],
        [0.045, alt + 1.10, -0.98],
        STEEL2,
        0.0,
    );
    push_box(
        &mut m,
        [-0.055, alt + 0.86, -1.30],
        [0.055, alt + 1.02, -1.06],
        TEAM,
        1.0,
    );
    for sx in [-1.0_f32, 1.0] {
        let (x0, x1) = (sx * 0.28, sx * 0.62);
        push_box(
            &mut m,
            [x0.min(x1), alt + 0.48, -1.40],
            [x0.max(x1), alt + 0.56, -1.02],
            STEEL2,
            0.0,
        );
    }
    // Bomb bay doors under the belly.
    push_box(
        &mut m,
        [-0.20, alt - 0.06, -0.45],
        [0.20, alt + 0.02, 0.35],
        GUN,
        0.0,
    );
    // Landing skids.
    for sx in [-1.0_f32, 1.0] {
        push_box(
            &mut m,
            [sx * 0.40 - 0.05, 0.0, -0.6],
            [sx * 0.40 + 0.05, alt + 0.02, 0.4],
            DARK,
            0.0,
        );
    }
    m
}

// --------------------------------------------------- Astromancer buildings

/// A hanging grown root in two carved stages with a gold drip ring: the
/// shared underside of every hovering Astromancer structure (`r` scales it).
fn push_root(m: &mut Vec<UnitVertex>, r: f32) {
    push_frustum(
        m,
        0.0,
        0.0,
        r,
        r * 0.55,
        r * 0.40,
        r * 0.22,
        SHELL2,
        0.0,
        8,
        0.0,
        false,
    );
    push_frustum(
        m,
        0.0,
        0.0,
        r * 0.55,
        r * 0.10,
        r * 0.22,
        r * 0.11,
        SHELL3,
        0.0,
        8,
        0.0,
        false,
    );
    push_prism(
        m,
        0.0,
        0.0,
        r * 0.58,
        r * 0.21,
        r * 0.25,
        GOLD_DK,
        0.0,
        8,
        0.0,
        false,
    );
}

/// Athenaeum: the caster college (tier 2). A hovering plaza carrying the
/// lecture dome with a glowing scriptorium band, ringed by three school
/// spires, with rune shards orbiting and the team core gem at the crown.
/// ~10 wide, gate toward +z.
pub fn athenaeum_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_root(&mut m, 3.4);
    // Plaza slab with a gold ceremonial belt.
    push_frustum(
        &mut m, 0.0, 0.0, 4.6, 4.1, 1.35, 2.6, SHELL, 0.0, 8, 0.0, false,
    );
    push_prism(&mut m, 0.0, 0.0, 4.2, 2.6, 3.0, SHELL2, 0.0, 8, 0.0, true);
    push_prism(&mut m, 0.0, 0.0, 4.28, 2.2, 2.55, GOLD, 0.0, 8, 0.0, false);
    // Arched gate on the +z face, framed in gold over a glowing door.
    push_box(&mut m, [-1.2, 1.35, 3.6], [1.2, 3.4, 4.3], GOLD_DK, 0.0);
    push_box(&mut m, [-0.9, 1.35, 4.0], [0.9, 3.1, 4.36], AETHER, 0.0);
    // The lecture dome with its scriptorium band.
    push_frustum(
        &mut m, 0.0, 0.0, 2.9, 2.2, 3.0, 5.0, SHELL, 0.0, 8, 0.0, false,
    );
    push_prism(&mut m, 0.0, 0.0, 2.95, 3.6, 4.2, VIOLET, 0.0, 8, 0.0, false);
    push_frustum(
        &mut m, 0.0, 0.0, 2.2, 1.0, 5.0, 6.4, SHELL2, 0.0, 8, 0.0, false,
    );
    push_pyramid(&mut m, 0.0, 0.0, 1.0, 6.4, 8.0, GOLD, 0.0, 8, 0.0);
    push_gem(&mut m, 0.0, 0.0, 0.45, 8.1, 8.5, 9.2, TEAM, 1.0, 6);
    // Three school spires around the dome.
    for k in 0..3 {
        let a = std::f32::consts::TAU * k as f32 / 3.0 + std::f32::consts::FRAC_PI_2;
        let (sx, sz) = (a.cos() * 3.4, a.sin() * 3.4);
        push_prism(&mut m, sx, sz, 0.45, 3.0, 6.2, SHELL2, 0.0, 6, 0.0, false);
        push_prism(&mut m, sx, sz, 0.50, 4.6, 5.0, AETHER, 0.0, 6, 0.0, false);
        push_pyramid(&mut m, sx, sz, 0.50, 6.2, 7.5, GOLD, 0.0, 6, 0.0);
    }
    // Orbiting rune shards at uneven heights.
    for (k, &(sx, sz)) in [(-4.2_f32, 1.8_f32), (4.3, 1.2), (-1.2, -4.6), (2.6, -3.9)]
        .iter()
        .enumerate()
    {
        let y = 4.2 + 0.5 * (k % 3) as f32;
        push_gem(&mut m, sx, sz, 0.32, y - 0.45, y, y + 0.6, SHELL, 0.0, 6);
        push_pyramid(&mut m, sx, sz, 0.2, y + 0.7, y + 1.25, TEAM, 1.0, 6, 0.0);
    }
    m
}

/// Storm-Ward: the Astromancer static anti-air (tier 2). A Ward-school
/// levitating monolith crowned by a charged storm coil instead of the
/// firing prong: alternating dark and charged discs under a team crystal,
/// with bolt shards orbiting the crown. ~5 wide.
pub fn storm_ward_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let rock = [0.52, 0.51, 0.48];
    let rock_dk = [0.38, 0.37, 0.35];
    // Grounded anchor pad (the monolith hovers above it).
    push_frustum(
        &mut m, 0.0, 0.0, 1.8, 1.2, 0.0, 0.4, rock_dk, 0.0, 8, 0.0, true,
    );
    for k in 0..4 {
        let a = std::f32::consts::TAU * k as f32 / 4.0 + 0.4;
        push_gem(
            &mut m,
            a.cos() * 1.45,
            a.sin() * 1.45,
            0.13,
            0.30,
            0.45,
            0.64,
            GOLD_DK,
            0.0,
            4,
        );
    }
    // The hovering obelisk, gold-banded, with charged rune slits.
    push_frustum(
        &mut m, 0.0, 0.0, 1.5, 1.9, 1.3, 1.8, rock_dk, 0.0, 6, 0.0, false,
    );
    push_frustum(
        &mut m, 0.0, 0.0, 1.9, 1.1, 1.8, 4.6, rock, 0.0, 6, 0.0, false,
    );
    push_prism(&mut m, 0.0, 0.0, 1.62, 2.5, 2.9, GOLD, 0.0, 6, 0.0, false);
    for k in 0..3 {
        let a = std::f32::consts::TAU * k as f32 / 3.0 + 0.5;
        let (rx, rz) = (a.cos() * 1.34, a.sin() * 1.34);
        push_box(
            &mut m,
            [rx - 0.09, 3.2, rz - 0.09],
            [rx + 0.09, 4.1, rz + 0.09],
            STORM,
            0.0,
        );
    }
    // The storm coil: alternating dark and charged discs.
    for k in 0..3 {
        let y = 4.6 + k as f32 * 0.55;
        push_prism(
            &mut m,
            0.0,
            0.0,
            0.95 - 0.08 * k as f32,
            y,
            y + 0.25,
            rock_dk,
            0.0,
            8,
            0.0,
            false,
        );
        push_prism(
            &mut m,
            0.0,
            0.0,
            0.78 - 0.08 * k as f32,
            y + 0.25,
            y + 0.55,
            STORM,
            0.0,
            8,
            0.0,
            false,
        );
    }
    // Team crystal crown with orbiting bolt shards.
    push_gem(&mut m, 0.0, 0.0, 0.55, 6.25, 6.7, 7.6, TEAM, 1.0, 6);
    for k in 0..3 {
        let a = std::f32::consts::TAU * k as f32 / 3.0 + 0.2;
        push_gem(
            &mut m,
            a.cos() * 1.25,
            a.sin() * 1.25,
            0.12,
            5.6,
            5.8,
            6.05,
            STORM,
            0.0,
            4,
        );
    }
    m
}

/// Crucible: heavy-ground production (tier 2). A grown forge bowl over a
/// molten heart, with three horn spires channeling the heat and a gold
/// ceremonial band; the team channel rides a floating crest gem. ~9 wide.
pub fn crucible_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_root(&mut m, 3.2);
    // The bowl: outer flare, gold belt, and the inner lip.
    push_frustum(
        &mut m, 0.0, 0.0, 3.1, 4.4, 1.3, 3.4, SHELL, 0.0, 8, 0.0, false,
    );
    push_prism(&mut m, 0.0, 0.0, 4.46, 2.2, 2.7, GOLD, 0.0, 8, 0.0, false);
    push_frustum(
        &mut m, 0.0, 0.0, 4.4, 3.3, 3.4, 3.7, SHELL2, 0.0, 8, 0.0, false,
    );
    // The melt pool (flat shaded, like liquid light).
    let pool = m.len();
    push_prism(&mut m, 0.0, 0.0, 3.3, 3.4, 3.55, EMBER, 0.0, 8, 0.0, true);
    push_prism(&mut m, 0.0, 0.0, 1.8, 3.45, 3.7, FLAME, 0.0, 8, 0.0, true);
    set_detail(&mut m[pool..], DETAIL_FLAT);
    // Three horn spires around the rim.
    for k in 0..3 {
        let a = std::f32::consts::TAU * k as f32 / 3.0 + 0.5;
        let (sx, sz) = (a.cos() * 3.9, a.sin() * 3.9);
        push_frustum(
            &mut m, sx, sz, 0.55, 0.30, 2.6, 5.2, SHELL2, 0.0, 5, 0.0, false,
        );
        push_pyramid(&mut m, sx, sz, 0.30, 5.2, 6.3, EMBER, 0.0, 5, 0.0);
    }
    // The crest: a team gem floating over the melt.
    push_gem(&mut m, 0.0, 0.0, 0.5, 5.4, 5.85, 6.7, TEAM, 1.0, 6);
    m
}

/// Conservatory: the research dome (tier 2). Tiered scroll-discs with
/// glowing seams, a crowning team crystal, and two lectern pylons with
/// drifting rune gems. ~8 wide.
pub fn conservatory_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_root(&mut m, 2.9);
    // Three shrinking tiers, each sealed with an aether seam.
    let tiers = [
        (3.4_f32, 1.16_f32, 2.5_f32),
        (2.6, 3.85, 1.45),
        (1.8, 5.5, 1.1),
    ];
    for (r, y0, h) in tiers {
        push_prism(&mut m, 0.0, 0.0, r, y0, y0 + h, SHELL, 0.0, 8, 0.0, true);
        push_prism(
            &mut m,
            0.0,
            0.0,
            r * 0.88 + 0.12,
            y0 + h,
            y0 + h + 0.19,
            AETHER,
            0.0,
            8,
            0.0,
            false,
        );
    }
    // Crowning team crystal on a gold collar.
    push_prism(&mut m, 0.0, 0.0, 0.9, 6.79, 7.05, GOLD, 0.0, 8, 0.0, true);
    push_gem(&mut m, 0.0, 0.0, 0.55, 7.1, 7.7, 8.8, TEAM, 1.0, 6);
    // Lectern pylons with drifting rune gems.
    for sx in [-1.0_f32, 1.0] {
        push_prism(
            &mut m,
            sx * 3.8,
            0.0,
            0.42,
            1.2,
            3.6,
            SHELL2,
            0.0,
            6,
            0.0,
            false,
        );
        push_pyramid(&mut m, sx * 3.8, 0.0, 0.42, 3.6, 4.5, GOLD, 0.0, 6, 0.0);
        push_gem(&mut m, sx * 3.8, 0.0, 0.2, 4.8, 5.0, 5.3, VIOLET, 0.0, 4);
    }
    m
}

/// Aerie: air production (tier 3). A tall roost spire with two cantilevered
/// perch discs ringed in aether landing light, a gold crown, and the team
/// core running up the middle. ~12 tall.
pub fn aerie_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_root(&mut m, 3.0);
    // Base drum and the tapering roost spire.
    push_frustum(
        &mut m, 0.0, 0.0, 3.3, 2.4, 1.2, 2.4, SHELL, 0.0, 8, 0.0, false,
    );
    push_prism(&mut m, 0.0, 0.0, 2.55, 1.6, 2.0, GOLD, 0.0, 8, 0.0, false);
    push_frustum(
        &mut m, 0.0, 0.0, 1.7, 0.8, 2.4, 9.6, SHELL2, 0.0, 6, 0.0, false,
    );
    push_pyramid(&mut m, 0.0, 0.0, 0.9, 9.6, 11.6, GOLD, 0.0, 6, 0.0);
    // The team core seam up the spire and the beacon gem at the tip.
    push_prism(&mut m, 0.0, 0.0, 0.55, 2.4, 10.0, TEAM, 1.0, 6, 0.0, false);
    push_gem(&mut m, 0.0, 0.0, 0.35, 11.7, 12.0, 12.5, TEAM, 1.0, 6);
    // Two cantilevered perch discs with aether landing rings.
    for (sx, py) in [(-1.0_f32, 4.4_f32), (1.0, 6.8)] {
        let cx = sx * 2.6;
        push_box(
            &mut m,
            [cx.min(0.0), py - 0.3, -0.4],
            [cx.max(0.0), py, 0.4],
            SHELL3,
            0.0,
        );
        push_prism(
            &mut m,
            cx,
            0.0,
            1.5,
            py - 0.22,
            py,
            SHELL,
            0.0,
            8,
            0.0,
            true,
        );
        push_prism(
            &mut m,
            cx,
            0.0,
            1.28,
            py,
            py + 0.12,
            AETHER,
            0.0,
            8,
            0.0,
            false,
        );
    }
    m
}

/// Ley Nexus: the superweapon site (tier 3). A terraced mana well with a
/// glowing ley pool, votive gold stones, and the great team heart-crystal
/// suspended on a beam of light. ~11 wide.
pub fn ley_nexus_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_root(&mut m, 3.6);
    // Terraces stepping in toward the pool.
    push_frustum(
        &mut m, 0.0, 0.0, 5.0, 4.2, 1.45, 2.3, SHELL, 0.0, 8, 0.0, false,
    );
    push_frustum(
        &mut m, 0.0, 0.0, 3.7, 3.0, 2.3, 3.1, SHELL2, 0.0, 8, 0.0, true,
    );
    push_prism(&mut m, 0.0, 0.0, 4.3, 2.05, 2.4, GOLD, 0.0, 8, 0.0, false);
    // The ley pool (flat shaded liquid light) and the font column.
    let pool = m.len();
    push_prism(&mut m, 0.0, 0.0, 2.5, 3.1, 3.3, AETHER, 0.0, 10, 0.0, true);
    set_detail(&mut m[pool..], DETAIL_FLAT);
    push_frustum(
        &mut m, 0.0, 0.0, 1.1, 0.6, 3.3, 4.9, SHELL2, 0.0, 6, 0.0, true,
    );
    // The beam and the great heart-crystal.
    let beam = m.len();
    push_prism(&mut m, 0.0, 0.0, 0.22, 4.9, 6.6, AETHER, 0.0, 6, 0.0, false);
    set_detail(&mut m[beam..], DETAIL_FLAT);
    push_gem(&mut m, 0.0, 0.0, 1.15, 6.4, 7.6, 9.6, TEAM, 1.0, 6);
    // Votive gold stones around the terrace.
    for k in 0..5 {
        let a = std::f32::consts::TAU * k as f32 / 5.0 + 0.3;
        let (sx, sz) = (a.cos() * 3.4, a.sin() * 3.4);
        push_gem(
            &mut m,
            sx,
            sz,
            0.26,
            3.0,
            3.35,
            3.85 + 0.2 * (k % 2) as f32,
            GOLD,
            0.0,
            4,
        );
    }
    m
}

// ----------------------------------------------------- Hollowmen buildings

/// A grounded foundation: slab, chamfered curb, and a chevroned hazard
/// skirt - the shared base of the Hollowmen kit.
fn push_pad(m: &mut Vec<UnitVertex>, hx: f32, hz: f32, body_h: f32) {
    push_box(
        m,
        [-hx - 0.4, 0.0, -hz - 0.4],
        [hx + 0.4, 0.5, hz + 0.4],
        DARK,
        0.0,
    );
    push_box(
        m,
        [-hx - 0.2, 0.5, -hz - 0.2],
        [hx + 0.2, 0.62, hz + 0.2],
        STEEL3,
        0.0,
    );
    push_box(m, [-hx, 0.5, -hz], [hx, body_h, hz], STEEL, 0.0);
    let n = 6;
    for k in 0..n {
        let x0 = -hx - 0.04 + k as f32 * (2.0 * (hx + 0.04) / n as f32);
        let c = if k % 2 == 0 { HAZ } else { DARK };
        push_box(
            m,
            [x0, 0.62, -hz - 0.04],
            [x0 + 2.0 * (hx + 0.04) / n as f32, 1.0, hz + 0.04],
            c,
            0.0,
        );
    }
}

/// Arsenal: the munitions plant (tier 2). A sawtooth-roofed hall with a
/// team window band and blast door, a raised missile rack on the annex
/// roof, and a banded stack. ~11 wide, door toward +z.
pub fn arsenal_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_pad(&mut m, 5.0, 3.8, 3.4);
    // Sawtooth roof with skylight strips.
    for cx in [-3.2_f32, 0.0] {
        push_roof(&mut m, cx, 0.0, 1.6, 4.0, 3.4, 1.2, STEEL2);
        push_box(
            &mut m,
            [cx - 0.8, 3.95, -2.9],
            [cx - 0.3, 4.3, 2.9],
            AMBER,
            0.0,
        );
    }
    // Blast door (+z) with hazard jambs and the team window band.
    push_box(&mut m, [-1.6, 0.0, 3.7], [1.6, 2.6, 3.9], STEEL3, 0.0);
    for k in 0..3 {
        let y0 = 0.2 + k as f32 * 0.74;
        push_box(&mut m, [-1.3, y0, 3.82], [1.3, y0 + 0.6, 4.0], GUN, 0.0);
    }
    for sx in [-1.0_f32, 1.0] {
        push_box(
            &mut m,
            [sx * 1.8 - 0.16, 0.0, 3.8],
            [sx * 1.8 + 0.16, 2.6, 4.0],
            HAZ,
            0.0,
        );
    }
    push_box(&mut m, [-4.6, 2.2, 3.78], [-2.4, 2.9, 3.92], TEAM, 1.0);
    // Annex with the raised missile rack: three rounds on a stepped cradle.
    push_box(&mut m, [2.2, 3.4, -3.0], [4.8, 4.4, 3.0], STEEL3, 0.0);
    for k in 0..3 {
        let y0 = 4.4 + 0.5 * k as f32;
        let z0 = -1.8 + 0.5 * k as f32;
        push_box(&mut m, [2.7, y0, z0], [4.3, y0 + 0.24, z0 + 2.6], GUN, 0.0);
        for bx in [3.05_f32, 3.95] {
            push_prism(
                &mut m,
                bx,
                z0 + 2.4,
                0.18,
                y0 + 0.24,
                y0 + 0.5,
                STEEL2,
                0.0,
                6,
                0.0,
                false,
            );
            push_gem(
                &mut m,
                bx,
                z0 + 2.4,
                0.14,
                y0 + 0.42,
                y0 + 0.52,
                y0 + 0.78,
                [0.92, 0.30, 0.22],
                0.0,
                6,
            );
        }
    }
    // Banded stack and yard crates.
    push_prism(
        &mut m, -4.0, -2.6, 0.55, 3.4, 6.4, STEEL2, 0.0, 10, 0.0, true,
    );
    push_prism(&mut m, -4.0, -2.6, 0.6, 5.1, 5.5, HAZ, 0.0, 10, 0.0, false);
    push_box(&mut m, [3.4, 0.0, -5.0], [4.8, 1.1, -3.9], HAZ, 0.0);
    push_box(&mut m, [1.9, 0.0, -5.2], [3.2, 0.9, -4.1], STEEL2, 0.0);
    m
}

/// Bunker: the garrison defense (tier 1). A low cast-concrete blockhouse
/// with a lit firing slit and gun toward +z, a sandbag apron, a team plate
/// over the slit, and a comms antenna. ~6 wide.
pub fn bunker_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    // Berm, body, and the heavy roof slab.
    push_frustum(
        &mut m,
        0.0,
        0.0,
        4.1,
        3.4,
        0.0,
        0.6,
        CONCRETE2,
        0.0,
        4,
        std::f32::consts::FRAC_PI_4,
        false,
    );
    push_box(&mut m, [-2.5, 0.4, -2.0], [2.5, 1.9, 2.0], CONCRETE, 0.0);
    push_box(&mut m, [-2.7, 1.9, -2.2], [2.7, 2.4, 2.2], CONCRETE2, 0.0);
    // Firing slit with the gun run out, and the team plate above it.
    push_box(&mut m, [-1.6, 1.0, 1.98], [1.6, 1.4, 2.06], GUN, 0.0);
    push_box(&mut m, [-1.5, 1.06, 2.04], [1.5, 1.34, 2.1], AMBER, 0.0);
    push_tube_z(&mut m, 0.0, 1.2, 0.10, 2.06, 3.2, GUN, 0.0, 6, true);
    push_box(&mut m, [-0.9, 1.5, 1.98], [0.9, 1.85, 2.08], TEAM, 1.0);
    // Sandbag apron in two staggered courses.
    for k in 0..5 {
        let x0 = -2.3 + k as f32 * 1.0;
        push_box(
            &mut m,
            [x0, 0.0, 2.3],
            [x0 + 0.9, 0.5, 3.0],
            [0.47, 0.44, 0.34],
            0.0,
        );
        if k < 4 {
            push_box(
                &mut m,
                [x0 + 0.5, 0.5, 2.4],
                [x0 + 1.4, 0.95, 2.9],
                [0.53, 0.49, 0.38],
                0.0,
            );
        }
    }
    // Roof details: hatch, vent, antenna.
    push_box(&mut m, [0.9, 2.4, -1.2], [1.9, 2.7, -0.2], STEEL3, 0.0);
    push_box(&mut m, [-1.8, 2.4, 0.3], [-1.0, 2.65, 1.1], DARK, 0.0);
    push_prism(&mut m, -2.2, -1.7, 0.05, 2.4, 4.2, GUN, 0.0, 6, 0.0, false);
    push_box(
        &mut m,
        [-2.26, 4.2, -1.76],
        [-2.14, 4.34, -1.64],
        AMBER,
        0.0,
    );
    m
}

/// Flak Tower: the static anti-air (tier 2). A concrete shaft up to a gun
/// platform with a quad mount stepped skyward, a spotting dish, and a team
/// band around the platform rim. ~6 wide, ~8 tall.
pub fn flak_tower_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    // Footing and the shaft, with a hazard band partway up.
    push_box(&mut m, [-2.4, 0.0, -2.4], [2.4, 0.9, 2.4], CONCRETE2, 0.0);
    push_box(&mut m, [-1.5, 0.9, -1.5], [1.5, 5.4, 1.5], CONCRETE, 0.0);
    push_box(&mut m, [-1.54, 2.0, -1.54], [1.54, 2.5, 1.54], HAZ, 0.0);
    // Gun platform with the team band on its rim.
    push_box(&mut m, [-2.2, 5.4, -2.2], [2.2, 6.1, 2.2], DARK, 0.0);
    push_box(&mut m, [-2.24, 5.62, -2.24], [2.24, 5.94, 2.24], TEAM, 1.0);
    // The quad mount: housing and four stepped barrels.
    push_box(&mut m, [-1.0, 6.1, -1.0], [1.0, 7.0, 1.0], STEEL2, 0.0);
    for sx in [-0.55_f32, 0.55] {
        for sz in [-0.45_f32, 0.45] {
            for k in 0..3 {
                let y0 = 7.0 + 0.55 * k as f32;
                let z0 = sz + 0.30 * k as f32;
                push_box(
                    &mut m,
                    [sx - 0.1, y0, z0 - 0.1],
                    [sx + 0.1, y0 + 0.6, z0 + 0.2],
                    GUN,
                    0.0,
                );
            }
        }
    }
    // Spotting dish and a klaxon on the platform corners.
    push_frustum(
        &mut m, -1.7, -1.7, 0.12, 0.5, 6.1, 6.45, STEEL2, 0.0, 8, 0.0, false,
    );
    push_prism(
        &mut m, -1.7, -1.7, 0.09, 6.1, 6.6, SENSOR, 0.0, 6, 0.0, true,
    );
    push_box(&mut m, [1.5, 6.1, 1.5], [1.8, 6.5, 1.8], AMBER, 0.0);
    m
}

/// Machine Shop: the Factory add-on (tier 2). A small gabled annex with a
/// crane mast over the yard, a leaning spare gear, a team window strip, and
/// a shutter door toward +z. ~7 wide.
pub fn machine_shop_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_pad(&mut m, 3.2, 2.4, 2.6);
    push_roof(&mut m, -0.6, 0.0, 2.6, 2.6, 2.6, 1.0, STEEL2);
    push_box(&mut m, [-2.6, 3.2, -1.4], [-0.6, 3.9, 1.4], STEEL3, 0.0); // roof house
                                                                        // Shutter door (+z) with hazard jambs, and the team window strip.
    push_box(&mut m, [-1.3, 0.0, 2.3], [1.3, 2.1, 2.5], STEEL3, 0.0);
    for k in 0..3 {
        let y0 = 0.15 + k as f32 * 0.6;
        push_box(&mut m, [-1.05, y0, 2.42], [1.05, y0 + 0.48, 2.6], GUN, 0.0);
    }
    push_box(&mut m, [1.7, 1.5, 2.38], [3.0, 2.1, 2.52], TEAM, 1.0);
    // Crane: mast, jib with a hazard stripe, and the hook cable.
    push_box(&mut m, [3.2, 0.0, -0.8], [3.8, 4.6, -0.2], GUN, 0.0);
    push_box(&mut m, [2.4, 4.6, -0.7], [6.0, 5.0, -0.3], HAZ, 0.0);
    push_box(&mut m, [5.3, 3.0, -0.56], [5.5, 4.6, -0.44], GUN, 0.0);
    push_box(&mut m, [5.1, 2.6, -0.66], [5.7, 3.0, -0.34], STEEL3, 0.0);
    // The spare gear leaned against the wall: a toothed disc on edge.
    push_prism(&mut m, 4.6, 1.4, 1.0, 0.0, 0.4, STEEL2, 0.0, 8, 0.0, true);
    for k in 0..8 {
        let a = std::f32::consts::TAU * k as f32 / 8.0;
        let (gx, gz) = (4.6 + a.cos() * 1.15, 1.4 + a.sin() * 1.15);
        push_box(
            &mut m,
            [gx - 0.14, 0.0, gz - 0.14],
            [gx + 0.14, 0.36, gz + 0.14],
            STEEL2,
            0.0,
        );
    }
    push_prism(&mut m, 4.6, 1.4, 0.3, 0.0, 0.46, DARK, 0.0, 8, 0.0, true);
    m
}

/// Radar Array: detection (tier 2). An ops hut with a team window beside a
/// staged lattice mast carrying the main dish, its glowing feed aimed
/// skyward, plus a small spotter dish. ~8 wide.
pub fn radar_array_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_pad(&mut m, 3.6, 2.4, 0.62);
    // Ops hut with the team window band.
    push_box(&mut m, [-3.2, 0.5, -1.9], [-0.4, 2.4, 1.9], STEEL, 0.0);
    push_box(&mut m, [-2.9, 1.5, 1.86], [-0.7, 2.0, 1.98], TEAM, 1.0);
    push_box(&mut m, [-2.6, 2.4, -1.2], [-1.0, 2.9, 1.2], STEEL2, 0.0);
    // Spotter dish on the hut roof.
    push_frustum(
        &mut m, -1.8, 0.0, 0.14, 0.6, 2.9, 3.3, STEEL2, 0.0, 8, 0.0, false,
    );
    push_box(&mut m, [-1.92, 3.3, -0.12], [-1.68, 3.55, 0.12], AMBER, 0.0);
    // The mast in three narrowing stages.
    push_box(&mut m, [0.8, 0.5, -1.1], [3.0, 1.1, 1.1], CONCRETE2, 0.0);
    push_box(&mut m, [1.3, 1.1, -0.6], [2.5, 3.4, 0.6], DARK, 0.0);
    push_box(&mut m, [1.5, 3.4, -0.4], [2.3, 5.4, 0.4], GUN, 0.0);
    // Main dish opening skyward with its glowing feed.
    push_frustum(
        &mut m, 1.9, 0.0, 0.4, 1.9, 5.4, 6.4, STEEL2, 0.0, 8, 0.0, false,
    );
    push_prism(&mut m, 1.9, 0.0, 0.22, 5.4, 6.9, SENSOR, 0.0, 6, 0.0, true);
    push_box(
        &mut m,
        [1.78, 6.9, -0.12],
        [2.02, 7.15, 0.12],
        [0.92, 0.30, 0.22],
        0.0,
    );
    m
}

/// Starport: air production (tier 3). A lit landing apron with approach
/// strips, the control tower with a team window band, and a fuel farm with
/// banded tanks. ~13 wide, pad open toward +z.
pub fn starport_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_pad(&mut m, 6.0, 4.8, 0.7);
    // The landing pad with three lit approach strips.
    push_box(&mut m, [-3.4, 0.7, -3.6], [4.6, 0.95, 4.0], DARK, 0.0);
    for sx in [-2.2_f32, 0.4, 3.0] {
        push_box(
            &mut m,
            [sx - 0.25, 0.95, -3.0],
            [sx + 0.25, 1.05, 3.4],
            SENSOR,
            0.0,
        );
    }
    for (cx, cz) in [(-3.0_f32, -3.2_f32), (4.2, -3.2), (-3.0, 3.6), (4.2, 3.6)] {
        push_box(
            &mut m,
            [cx - 0.2, 0.95, cz - 0.2],
            [cx + 0.2, 1.2, cz + 0.2],
            AMBER,
            0.0,
        );
    }
    // Control tower with the mullioned team cab and a beacon.
    push_box(&mut m, [-5.8, 0.5, -4.4], [-3.9, 5.4, -2.5], STEEL, 0.0);
    push_box(&mut m, [-6.1, 5.4, -4.7], [-3.6, 6.4, -2.2], STEEL2, 0.0);
    push_box(&mut m, [-6.14, 5.7, -4.74], [-3.56, 6.2, -2.16], TEAM, 1.0);
    push_box(
        &mut m,
        [-5.0, 6.4, -3.6],
        [-4.7, 6.85, -3.3],
        [0.92, 0.30, 0.22],
        0.0,
    );
    // Fuel farm: banded tanks behind the pad.
    for (k, &(tx, tz)) in [(-5.2_f32, 1.2_f32), (-5.2, 3.2)].iter().enumerate() {
        let h = 2.6 - 0.3 * k as f32;
        push_prism(&mut m, tx, tz, 0.75, 0.5, h, STEEL2, 0.0, 10, 0.0, false);
        push_dome(&mut m, tx, tz, 0.75, h, STEEL3, 0.0, 10, 2);
        push_prism(&mut m, tx, tz, 0.79, 1.3, 1.7, HAZ, 0.0, 10, 0.0, false);
    }
    m
}

/// Fusion Reactor: the big power plant (tier 3). A containment dome with a
/// glowing core seam and team collar, two waisted cooling towers, coolant
/// trunks, and a switch house. ~13 wide.
pub fn fusion_reactor_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_pad(&mut m, 5.6, 4.0, 0.7);
    // Containment dome: stages, the cyan core seam, and a team collar.
    push_frustum(
        &mut m, 0.0, 0.0, 3.4, 2.9, 0.7, 1.9, CONCRETE, 0.0, 10, 0.0, false,
    );
    push_prism(
        &mut m, 0.0, 0.0, 2.95, 1.9, 2.5, SENSOR, 0.0, 10, 0.0, false,
    );
    push_prism(&mut m, 0.0, 0.0, 3.0, 2.5, 3.0, TEAM, 1.0, 10, 0.0, false);
    push_frustum(
        &mut m, 0.0, 0.0, 2.9, 1.9, 3.0, 4.4, STEEL, 0.0, 10, 0.0, false,
    );
    push_dome(&mut m, 0.0, 0.0, 1.9, 4.4, STEEL2, 0.0, 10, 3);
    push_prism(&mut m, 0.0, 0.0, 0.35, 6.1, 6.9, SENSOR, 0.0, 6, 0.0, true);
    // The two waisted cooling towers with crown rims.
    for sx in [-1.0_f32, 1.0] {
        let cx = sx * 4.3;
        push_frustum(
            &mut m, cx, 0.0, 1.5, 0.95, 0.7, 3.2, CONCRETE, 0.0, 10, 0.0, false,
        );
        push_frustum(
            &mut m, cx, 0.0, 0.95, 1.25, 3.2, 5.2, CONCRETE, 0.0, 10, 0.0, false,
        );
        push_prism(
            &mut m, cx, 0.0, 1.0, 5.2, 5.45, CONCRETE2, 0.0, 10, 0.0, false,
        );
        // Coolant trunk back to the dome.
        push_box(
            &mut m,
            [cx.min(0.0) + 0.8, 0.9, -0.25],
            [cx.max(0.0) - 0.8, 1.4, 0.25],
            GUN,
            0.0,
        );
    }
    // Switch house with an amber service door.
    push_box(&mut m, [-1.6, 0.7, 2.6], [1.0, 2.0, 3.9], STEEL, 0.0);
    push_box(&mut m, [-0.9, 0.7, 3.9], [0.3, 1.7, 3.98], AMBER, 0.0);
    m
}

/// Drydock: capital construction (tier 3). Four gantry legs carrying hazard
/// rails and a traveling crane over a capital hull mid-assembly: plated and
/// team-striped aft, bare frames at the bow, keel blocks under. ~13 wide.
pub fn drydock_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_pad(&mut m, 6.0, 4.2, 0.62);
    // Gantry legs and rails.
    for sz in [-1.0_f32, 1.0] {
        for sx in [-1.0_f32, 1.0] {
            push_box(
                &mut m,
                [sx * 4.6 - 0.45, 0.5, sz * 3.2 - 0.45],
                [sx * 4.6 + 0.45, 7.2, sz * 3.2 + 0.45],
                DARK,
                0.0,
            );
        }
        push_box(
            &mut m,
            [-5.2, 7.2, sz * 3.2 - 0.35],
            [5.2, 7.9, sz * 3.2 + 0.35],
            HAZ,
            0.0,
        );
    }
    // Traveling crane bridge with the hook cable.
    push_box(&mut m, [-1.2, 7.9, -3.5], [1.2, 8.6, 3.5], GUN, 0.0);
    push_box(&mut m, [-0.15, 5.2, -0.8], [0.15, 7.9, -0.5], GUN, 0.0);
    push_box(&mut m, [-0.5, 4.7, -1.0], [0.5, 5.2, -0.3], STEEL3, 0.0);
    // The hull on keel blocks: plated aft with a team stripe, bare bow.
    for bx in [-2.8_f32, 0.0, 2.8] {
        push_box(
            &mut m,
            [bx - 0.5, 0.6, -0.9],
            [bx + 0.5, 1.5, 0.9],
            CONCRETE2,
            0.0,
        );
    }
    push_box(&mut m, [-4.2, 1.5, -1.7], [1.4, 4.0, 1.7], STEEL, 0.0);
    push_box(&mut m, [-4.2, 2.6, -1.74], [1.4, 3.1, 1.74], TEAM, 1.0);
    push_box(&mut m, [1.4, 1.7, -1.4], [3.0, 3.7, 1.4], DARK, 0.0);
    push_box(&mut m, [3.0, 1.9, -1.0], [4.1, 3.4, 1.0], DARK, 0.0);
    push_box(&mut m, [-3.8, 3.5, 1.7], [-0.6, 3.8, 1.78], AMBER, 0.0);
    m
}

/// Missile Silo: the nuke site (tier 3). A hardened apron with the blast
/// doors swung open, the missile nosing out of the shaft with a team body
/// band and a red ogive, klaxon studs around the collar, and an ops bunker.
/// ~11 wide.
pub fn missile_silo_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_pad(&mut m, 4.6, 4.6, 0.9);
    // The silo collar and shaft mouth.
    push_prism(
        &mut m, 0.0, 0.0, 2.6, 0.9, 1.5, CONCRETE, 0.0, 10, 0.0, true,
    );
    push_prism(&mut m, 0.0, 0.0, 1.9, 0.9, 1.56, GUN, 0.0, 10, 0.0, true);
    // Blast doors laid open either side, hazard-edged.
    for sx in [-1.0_f32, 1.0] {
        let (x0, x1) = (sx * 2.7, sx * 4.5);
        push_box(
            &mut m,
            [x0.min(x1), 0.9, -1.9],
            [x0.max(x1), 1.3, 1.9],
            STEEL2,
            0.0,
        );
        push_box(
            &mut m,
            [x0.min(x1), 1.3, -1.9],
            [x0.max(x1), 1.4, -1.4],
            HAZ,
            0.0,
        );
    }
    // The missile: body, team band, tapering shoulder, red ogive.
    push_prism(
        &mut m, 0.0, 0.0, 0.95, 1.0, 4.6, STEEL2, 0.0, 10, 0.0, false,
    );
    push_prism(&mut m, 0.0, 0.0, 0.99, 2.5, 3.1, TEAM, 1.0, 10, 0.0, false);
    push_frustum(
        &mut m, 0.0, 0.0, 0.95, 0.45, 4.6, 6.0, STEEL, 0.0, 10, 0.0, false,
    );
    push_pyramid(
        &mut m,
        0.0,
        0.0,
        0.45,
        6.0,
        7.2,
        [0.92, 0.30, 0.22],
        0.0,
        10,
        0.0,
    );
    // Klaxon studs around the collar.
    for k in 0..4 {
        let a = std::f32::consts::TAU * k as f32 / 4.0 + std::f32::consts::FRAC_PI_4;
        push_box(
            &mut m,
            [a.cos() * 2.3 - 0.12, 1.5, a.sin() * 2.3 - 0.12],
            [a.cos() * 2.3 + 0.12, 1.78, a.sin() * 2.3 + 0.12],
            AMBER,
            0.0,
        );
    }
    // Ops bunker on the corner with a team window.
    push_box(&mut m, [2.4, 0.9, -4.3], [4.4, 2.2, -2.6], STEEL, 0.0);
    push_box(&mut m, [2.7, 1.5, -2.64], [4.1, 1.95, -2.56], TEAM, 1.0);
    m
}
