use crate::{Point, block_position, hostile, model::Flag, service::Service};
use mod_api::entity::health::{DamagePlayer, HurtSource};
use uuid::Uuid;

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
    origin.is_some_and(|(dim, pos)| denies_pvp_at(service, dim, pos))
        || denies_pvp_at(service, dimension, block_position(target))
}

pub fn denies_pvp_at(service: &Service, dimension: &str, target: Point) -> bool {
    service.store.is_some() && !service.natural(dimension, target, Flag::Pvp)
}

pub fn denies_entity_damage(
    service: &mut Service,
    dimension: &str,
    position: [f64; 3],
    kind: &str,
    living: bool,
    player: Option<&DamagePlayer>,
) -> bool {
    if service.store.is_none() {
        return false;
    }
    if kind == "minecraft:player" {
        return player.is_some_and(|player| {
            denies_pvp(
                service,
                Some((&player.dimension, block_position(player.position))),
                dimension,
                position,
            )
        });
    }
    let flag = target_flag(kind, living);
    let pos = block_position(position);
    let Some(player) = player else {
        return !living && !service.natural(dimension, pos, flag);
    };
    let actor = crate::service::Actor {
        uuid: Uuid::from_u128(player.uuid),
        name: player.name.clone(),
        operator: player.permission > 0,
    };
    let denied = !service.allowed(&actor, dimension, pos, flag);
    if denied {
        service.queue_denial(actor.uuid, flag);
    }
    denied
}
