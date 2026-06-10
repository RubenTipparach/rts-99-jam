//! Human-readable scenario maps.
//!
//! A `.map` file in `assets/maps/` bakes a battle's starting layout: bases,
//! starting units, and the neutral resource clusters. The client embeds the
//! file at compile time (so it loads identically native and on WASM) and
//! turns it into ordinary setup [`Command`]s, which flow through the same
//! deterministic sim path as every other command.
//!
//! Format (full spec in `docs/maps.md`): one entity per line.
//!
//! ```text
//! name <map name>
//! hq|barracks|turret     <player> <x> <z>
//! infantry|worker|heavy  <player> <x> <z>
//! ore|carbon             <x> <z>
//! ```
//!
//! `#` starts a comment, blank lines are skipped, and coordinates are integer
//! world units within the battlefield (`|x|, |z| <= terrain::HALF`).

use crate::terrain;
use math::Fx;
use protocol::{BuildingKind, Command, PlayerId, ResourceKind, UnitKind};

/// The default skirmish scenario, baked into the binary.
pub const SKIRMISH: &str = include_str!("../../../assets/maps/crossfire_basin.map");

/// A parsed map: its display name and the setup commands that realize it.
pub struct MapFile {
    pub name: String,
    pub commands: Vec<Command>,
}

fn coord(tok: &str, line: usize) -> Result<Fx, String> {
    let v: i32 = tok
        .parse()
        .map_err(|_| format!("line {line}: '{tok}' is not an integer coordinate"))?;
    let bound = terrain::HALF as i32;
    if v < -bound || v > bound {
        return Err(format!(
            "line {line}: coordinate {v} is outside the battlefield (-{bound}..{bound})"
        ));
    }
    Ok(Fx::from_int(v))
}

fn player(tok: &str, line: usize) -> Result<PlayerId, String> {
    tok.parse()
        .map_err(|_| format!("line {line}: '{tok}' is not a player id"))
}

/// Parse a map file into its name and setup commands.
pub fn parse(src: &str) -> Result<MapFile, String> {
    let mut name = None;
    let mut commands = Vec::new();
    for (idx, raw) in src.lines().enumerate() {
        let line = idx + 1;
        let text = raw.split('#').next().unwrap_or("").trim();
        if text.is_empty() {
            continue;
        }
        let mut tok = text.split_whitespace();
        let word = tok.next().unwrap_or("");
        let args: Vec<&str> = tok.collect();
        match word {
            "name" => {
                if args.is_empty() {
                    return Err(format!("line {line}: 'name' needs a value"));
                }
                name = Some(args.join(" "));
            }
            "hq" | "barracks" | "turret" | "infantry" | "worker" | "heavy" => {
                let [p, x, z] = args[..] else {
                    return Err(format!("line {line}: expected '{word} <player> <x> <z>'"));
                };
                let owner = player(p, line)?;
                let (x, y) = (coord(x, line)?, coord(z, line)?);
                commands.push(match word {
                    "hq" => Command::SpawnBuilding {
                        owner,
                        kind: BuildingKind::Hq,
                        x,
                        y,
                    },
                    "barracks" => Command::SpawnBuilding {
                        owner,
                        kind: BuildingKind::Barracks,
                        x,
                        y,
                    },
                    "turret" => Command::SpawnBuilding {
                        owner,
                        kind: BuildingKind::Turret,
                        x,
                        y,
                    },
                    "infantry" => Command::SpawnUnit {
                        owner,
                        kind: UnitKind::Infantry,
                        x,
                        y,
                    },
                    "worker" => Command::SpawnUnit {
                        owner,
                        kind: UnitKind::Worker,
                        x,
                        y,
                    },
                    _ => Command::SpawnUnit {
                        owner,
                        kind: UnitKind::Heavy,
                        x,
                        y,
                    },
                });
            }
            "ore" | "carbon" => {
                let [x, z] = args[..] else {
                    return Err(format!("line {line}: expected '{word} <x> <z>'"));
                };
                commands.push(Command::SpawnResource {
                    kind: if word == "ore" {
                        ResourceKind::Ore
                    } else {
                        ResourceKind::Carbon
                    },
                    x: coord(x, line)?,
                    y: coord(z, line)?,
                });
            }
            other => return Err(format!("line {line}: unknown directive '{other}'")),
        }
    }
    Ok(MapFile {
        name: name.ok_or("map has no 'name' line")?,
        commands,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(v: Fx) -> i64 {
        v.floor_int()
    }

    /// `(x, z)` of every spawned resource node.
    fn nodes(map: &MapFile) -> Vec<(i64, i64)> {
        map.commands
            .iter()
            .filter_map(|c| match *c {
                Command::SpawnResource { x, y, .. } => Some((int(x), int(y))),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn baked_skirmish_map_parses() {
        let map = parse(SKIRMISH).expect("baked map must parse");
        assert_eq!(map.name, "Crossfire Basin");
        let mut ore = 0;
        let mut carbon = 0;
        let mut hqs = 0;
        let mut workers = 0;
        let mut other = 0;
        for c in &map.commands {
            match c {
                Command::SpawnResource { kind, .. } => match kind {
                    ResourceKind::Ore => ore += 1,
                    ResourceKind::Carbon => carbon += 1,
                },
                Command::SpawnBuilding {
                    kind: BuildingKind::Hq,
                    ..
                } => hqs += 1,
                Command::SpawnUnit {
                    kind: UnitKind::Worker,
                    ..
                } => workers += 1,
                _ => other += 1,
            }
        }
        // Three mains (6 ore + 1 carbon each), four naturals (4 + 1), a rich
        // center (6 + 2), and two side clusters (4 + 1).
        assert_eq!(ore, 6 * 3 + 4 * 4 + 6 + 4 * 2);
        assert_eq!(carbon, 3 + 4 + 2 + 2);
        // StarCraft-style starts: each main is exactly an HQ + 4 workers;
        // nothing else (no free production or army) is on the map.
        assert_eq!(hqs, 3);
        assert_eq!(workers, 3 * 4);
        assert_eq!(other, 0);
    }

    #[test]
    fn baked_map_keeps_clearances() {
        // Mirrors the sim's footprints: node obstacle radius is 3.5 (so two
        // nodes need >= 8 between centers to leave a worker gap) and buildings
        // must keep the BUILD_CLEAR2 distance (14) from every node.
        let map = parse(SKIRMISH).unwrap();
        let nodes = nodes(&map);
        for (i, &(ax, az)) in nodes.iter().enumerate() {
            for &(bx, bz) in &nodes[i + 1..] {
                let d2 = (ax - bx).pow(2) + (az - bz).pow(2);
                assert!(d2 >= 8 * 8, "nodes at ({ax},{az}) and ({bx},{bz}) overlap");
            }
        }
        for c in &map.commands {
            if let Command::SpawnBuilding { x, y, .. } = *c {
                let (bx, bz) = (int(x), int(y));
                for &(nx, nz) in &nodes {
                    let d2 = (bx - nx).pow(2) + (bz - nz).pow(2);
                    assert!(
                        d2 >= 14 * 14,
                        "building at ({bx},{bz}) crowds the node at ({nx},{nz})"
                    );
                }
            }
        }
    }

    #[test]
    fn shared_clusters_are_mirrored() {
        // Contested clusters (everything between the main-base ore lines, i.e.
        // |z| < 190) must mirror both left/right and top/bottom, so neither
        // side has a shorter path to a shared patch. Main-base clusters are
        // exempt: a main's geyser sits on one flank of its ore line by design.
        let map = parse(SKIRMISH).unwrap();
        let nodes = nodes(&map);
        for &(x, z) in &nodes {
            if z.abs() >= 190 {
                continue;
            }
            assert!(
                nodes.contains(&(-x, z)),
                "contested node ({x},{z}) has no E/W mirror"
            );
            assert!(
                nodes.contains(&(x, -z)),
                "contested node ({x},{z}) has no N/S mirror"
            );
        }
    }

    #[test]
    fn rejects_malformed_lines() {
        assert!(parse("name X\nvolcano 0 1 2").is_err()); // unknown directive
        assert!(parse("name X\nore 10").is_err()); // missing coordinate
        assert!(parse("name X\nbarracks 0 10").is_err()); // missing coordinate
        assert!(parse("name X\nore 10 99999").is_err()); // off the battlefield
        assert!(parse("name X\ninfantry blue 0 0").is_err()); // bad player id
        assert!(parse("ore 1 2").is_err()); // no name line
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let map = parse("# header\n\nname A B\nore 1 2 # trailing\n").unwrap();
        assert_eq!(map.name, "A B");
        assert_eq!(map.commands.len(), 1);
    }
}
