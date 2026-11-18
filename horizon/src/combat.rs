use crate::{Point, block_position, hostile, model::Flag, service::Service};
use mod_api::entity::health::HurtSource;

// Impact dispatch and damage dispatch use these same server-wide IDs. Keep the
// types in the key as well so a later damage attempt cannot inherit the veto.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Shot {
    projectile: u32,
    owner: u32,
    target: u32,
    damage_type: &'static str,
}

impl Shot {
    pub fn at_impact(kind: &str, projectile: u32, owner: u32, target: u32) -> Option<Self> {
        if projectile == 0 || target == 0 {
            return None;
        }
        let damage_type = match kind {
            "minecraft:arrow" | "minecraft:spectral_arrow" => "minecraft:arrow",
            "minecraft:trident" => "minecraft:trident",
            _ => return None,
        };
        Some(Self {
            projectile,
            owner,
            target,
            damage_type,
        })
    }

    pub fn at_damage(target: i32, source: &HurtSource<'_>) -> Option<Self> {
        Self::at_impact(
            source.damage_type,
            source.direct as u32,
            source.causing as u32,
            target as u32,
        )
    }
}

pub fn target_flag(kind: &str, living: bool) -> Flag {
    if !living {
        Flag::Build
    } else if hostile(kind) {
        Flag::DamageMonster
    } else {
        Flag::DamageAnimals
    }
}

pub fn denies_pvp(
    service: &Service,
    origin: Option<(&str, Point)>,
    dimension: &str,
    target: [f64; 3],
) -> bool {
    service.store.is_some()
        && (origin.is_some_and(|(dim, pos)| !service.natural(dim, pos, Flag::Pvp))
            || !service.natural(dimension, block_position(target), Flag::Pvp))
}
