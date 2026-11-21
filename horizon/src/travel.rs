use crate::{
    Mod, actor,
    model::{Area, Flag, Point, Volume},
    service::Actor,
    store::Settings,
    tell,
};
use mod_api::{
    BlockShapeKind, ChunkRef, HostTransaction, Identifier, ItemPlace, ItemResource, ItemStack,
    LevelRef, PlayerRef, PlayerTeleportStatus, Registrar, ServerHost, Tag, TicketController,
    TicketOwner, ValidateTicketsEvent,
};
use std::{
    collections::{BTreeSet, HashMap},
    sync::{Mutex, OnceLock},
};
use uuid::Uuid;

const CONTROLLER: &str = "purtmarsarea:territory_travel";
const MAX_WAIT_TICKS: u64 = 100;
const HORIZONTAL_RADIUS: i32 = 4;
const VERTICAL_RADIUS: i32 = 6;

#[derive(Default)]
pub struct Travel {
    ticket_controller: OnceLock<TicketController>,
    pending: Mutex<HashMap<u128, Request>>,
}

struct Request {
    area: Uuid,
    level: LevelRef,
    chunks: Vec<ChunkRef>,
    expires: u64,
}

struct Destination {
    dimension: String,
    level: LevelRef,
    volume: Volume,
    origin: [f64; 3],
    name: String,
}

pub fn can_visit(actor: &Actor, area: &Area, settings: &Settings) -> bool {
    (area.is_owner(actor.uuid, &actor.name) || area.member(actor.uuid, &actor.name).is_some())
        && [Flag::Move, Flag::Teleport]
            .into_iter()
            .all(|flag| area.allows(flag, actor.uuid, &actor.name, settings.default_for(flag)))
}

impl Travel {
    pub fn register(&self, registrar: &mut dyn Registrar) {
        let _ = self
            .ticket_controller
            .set(registrar.register_ticket_controller(CONTROLLER, true));
    }

    pub fn validate(&self, event: &mut ValidateTicketsEvent<'_>) {
        if Some(&event.controller) == self.ticket_controller.get() {
            // Every ticket is temporary. A saved ticket came from an interrupted trip.
            for (uuid, _) in event.validation.entity_tickets() {
                event.validation.remove_entity_tickets(uuid);
            }
        }
    }

    pub fn start(
        &self,
        modification: &Mod,
        host: &mut dyn ServerHost,
        player: PlayerRef,
        actor: &Actor,
        area: Uuid,
    ) -> Result<(), String> {
        let uuid = player.uuid();
        if self.pending.lock().unwrap().contains_key(&uuid) {
            return Err("正在寻找上一次传送的安全落点".into());
        }
        let destination = destination(modification, host, player, actor, area)?;
        let scroll = scroll_in_inventory(host, player).ok_or("背包中需要 1 个领地传送符")?;
        if let Some(landing) = find_landing(host, &destination) {
            arrive(host, player, destination, landing, scroll)?;
            host.close_menu(uuid);
            return Ok(());
        }

        let mut chunks = BTreeSet::new();
        for x in centred(
            destination.volume.core_pos[0],
            destination.volume.min[0],
            destination.volume.max[0],
            HORIZONTAL_RADIUS,
        ) {
            for z in centred(
                destination.volume.core_pos[2],
                destination.volume.min[2],
                destination.volume.max[2],
                HORIZONTAL_RADIUS,
            ) {
                chunks.insert([x >> 4, z >> 4]);
            }
        }
        let chunks: Vec<_> = chunks
            .into_iter()
            .filter_map(|pos| destination.level.chunk_ref(host, pos))
            .filter(|chunk| !chunk.loaded(host))
            .collect();
        if chunks.is_empty() {
            return Err("领地核心附近找不到安全落点".into());
        }
        let controller = *self.ticket_controller.get().ok_or("无法加载目标所在区块")?;
        let owner = TicketOwner::Entity(uuid);
        let mut held = Vec::new();
        for chunk in chunks {
            if !chunk.set_mod_forced(host, controller, owner, true, false) {
                self.release(host, uuid, &held);
                return Err("无法加载目标所在区块".into());
            }
            held.push(chunk);
        }
        let expires = modification.service().ticks.saturating_add(MAX_WAIT_TICKS);
        self.pending.lock().unwrap().insert(
            uuid,
            Request {
                area,
                level: destination.level,
                chunks: held,
                expires,
            },
        );
        host.close_menu(uuid);
        tell(host, player, "正在加载领地并寻找安全落点……");
        Ok(())
    }

    pub fn tick(&self, modification: &Mod, host: &mut dyn ServerHost) {
        let requests = std::mem::take(&mut *self.pending.lock().unwrap());
        let now = modification.service().ticks;
        for (uuid, request) in requests {
            let player = host.player(uuid);
            if let Some(player) = player
                && player.level(host) == Some(request.level)
                && now < request.expires
                && !request.chunks.iter().all(|chunk| chunk.loaded(host))
            {
                self.pending.lock().unwrap().insert(uuid, request);
                continue;
            }
            if let Some(player) = player {
                let result = if player.level(host) != Some(request.level) {
                    Err("已离开原维度，传送取消".into())
                } else if now >= request.expires {
                    Err("加载领地区块超时".into())
                } else {
                    finish(modification, host, player, request.area)
                };
                if let Err(error) = result {
                    tell(host, player, error);
                }
            }
            self.release(host, uuid, &request.chunks);
        }
    }

    pub fn cancel(&self, host: &mut dyn ServerHost, uuid: u128) {
        let request = self.pending.lock().unwrap().remove(&uuid);
        if let Some(request) = request {
            self.release(host, uuid, &request.chunks);
        }
    }

    pub fn clear(&self, host: &mut dyn ServerHost) {
        let requests = std::mem::take(&mut *self.pending.lock().unwrap());
        for (uuid, request) in requests {
            self.release(host, uuid, &request.chunks);
        }
    }

    fn release(&self, host: &mut dyn ServerHost, uuid: u128, chunks: &[ChunkRef]) {
        if let Some(&controller) = self.ticket_controller.get() {
            for chunk in chunks {
                chunk.set_mod_forced(host, controller, TicketOwner::Entity(uuid), false, false);
            }
        }
    }
}

fn finish(
    modification: &Mod,
    host: &mut dyn ServerHost,
    player: PlayerRef,
    area: Uuid,
) -> Result<(), String> {
    let actor = actor(host, player).ok_or("无法读取玩家身份")?;
    let destination = destination(modification, host, player, &actor, area)?;
    let scroll = scroll_in_inventory(host, player).ok_or("背包中需要 1 个领地传送符")?;
    let landing = find_landing(host, &destination).ok_or("领地核心附近找不到安全落点")?;
    arrive(host, player, destination, landing, scroll)
}

fn destination(
    modification: &Mod,
    host: &dyn ServerHost,
    player: PlayerRef,
    actor: &Actor,
    id: Uuid,
) -> Result<Destination, String> {
    let level = player.level(host).ok_or("无法读取当前维度")?;
    let dimension = player
        .dimension(host)
        .ok_or("无法读取当前维度")?
        .to_string();
    let origin = player.entity().position(host).ok_or("无法读取玩家坐标")?;
    let service = modification.service();
    let config = service.ready()?;
    let area = service
        .index
        .areas
        .get(&id)
        .ok_or("领地已发生变化，请重新打开菜单")?;
    if area.world != service.world(&dimension) {
        return Err("只能传送到当前维度的领地".into());
    }
    if !can_visit(actor, area, &config.settings) {
        return Err("没有进入或传送到该领地的权限".into());
    }
    let in_area = service.at(&dimension, crate::block_position(origin));
    let can_leave = in_area.map_or_else(
        || {
            !config
                .settings
                .claim_world
                .get(service.world(&dimension))
                .is_some_and(|flags| flags.contains(&Flag::Teleport))
        },
        |area| {
            area.allows(
                Flag::Teleport,
                actor.uuid,
                &actor.name,
                config.settings.default_for(Flag::Teleport),
            )
        },
    );
    if !can_leave {
        return Err("当前位置不允许传送".into());
    }
    let volume = area.volumes.first().ok_or("领地没有主核心")?.clone();
    Ok(Destination {
        dimension,
        level,
        volume,
        origin,
        name: area.name.clone(),
    })
}

fn centred(centre: i32, min: i32, max: i32, radius: i32) -> impl Iterator<Item = i32> {
    (0..=radius * 2)
        .map(move |step| {
            centre
                + if step % 2 == 0 {
                    step / 2
                } else {
                    -(step + 1) / 2
                }
        })
        .filter(move |pos| (min..=max).contains(pos))
}

fn find_landing(host: &dyn ServerHost, destination: &Destination) -> Option<Point> {
    let level = destination.level;
    let height = level.dimension_shape(host)?;
    let [min_x, min_z, max_x, max_z] = level.world_border(host)?;
    let Volume {
        core_pos: [cx, cy, cz],
        min,
        max,
        ..
    } = destination.volume;
    let lowest = min[1].max(height.min_y + 1);
    let highest = max[1].min(height.min_y + height.height - 1) - 1;
    for x in centred(cx, min[0], max[0], HORIZONTAL_RADIUS) {
        for z in centred(cz, min[2], max[2], HORIZONTAL_RADIUS) {
            if f64::from(x) + 0.2 < min_x
                || f64::from(x) + 0.8 > max_x
                || f64::from(z) + 0.2 < min_z
                || f64::from(z) + 0.8 > max_z
                || level
                    .chunk_ref(host, [x >> 4, z >> 4])
                    .is_none_or(|chunk| !chunk.loaded(host))
            {
                continue;
            }
            for y in centred(cy, lowest, highest, VERTICAL_RADIUS) {
                let at = [x, y, z];
                if landing_is_safe(host, level, at) {
                    return Some(at);
                }
            }
        }
    }
    None
}

fn block_id(host: &dyn ServerHost, level: LevelRef, pos: Point) -> Option<String> {
    level.block(host, pos).map(|key| key.id(host))
}

fn safe_support(name: &str) -> bool {
    name.starts_with("minecraft:")
        && !matches!(
            name,
            "minecraft:magma_block"
                | "minecraft:cactus"
                | "minecraft:campfire"
                | "minecraft:soul_campfire"
                | "minecraft:powder_snow"
                | "minecraft:pointed_dripstone"
                | "minecraft:sweet_berry_bush"
                | "minecraft:fire"
                | "minecraft:soul_fire"
        )
}

fn landing_is_safe(host: &dyn ServerHost, level: LevelRef, [x, y, z]: Point) -> bool {
    let below = [x, y - 1, z];
    if ![y, y + 1].into_iter().all(|at| {
        matches!(
            block_id(host, level, [x, at, z]).as_deref(),
            Some("minecraft:air" | "minecraft:cave_air")
        )
    }) || !block_id(host, level, below).is_some_and(|id| safe_support(&id))
    {
        return false;
    }
    let Some(floor) = level.block_state(host, below) else {
        return false;
    };
    let Some(shape) = level.block_shape(host, floor, below, BlockShapeKind::Collision) else {
        return false;
    };
    // Require one flat shape supporting the whole standing footprint.
    if !shape.iter().any(|&[min_x, _, min_z, max_x, max_y, max_z]| {
        min_x <= 0.2 && min_z <= 0.2 && max_x >= 0.8 && max_z >= 0.8 && max_y == 1.0
    }) || shape.iter().any(|shape| shape[4] > 1.0)
    {
        return false;
    }
    let [x, y, z] = [x, y, z].map(f64::from);
    let bounds = [x + 0.2, y, z + 0.2, x + 0.8, y + 1.8, z + 0.8];
    // Both body cells are confirmed as air. The wider collision scan also
    // checks neighboring shapes when the player's level is in place.
    level.has_collisions(host, bounds) != Some(true)
}

fn scroll_in_inventory(
    host: &mut dyn ServerHost,
    player: PlayerRef,
) -> Option<(ItemPlace, ItemResource)> {
    let paper = host.item_key(&Identifier::parse("minecraft:paper").ok()?)?;
    (0..36).chain([-106]).find_map(|slot| {
        let place = ItemPlace::PlayerSlot {
            player: player.uuid(),
            slot,
        };
        let stack = place.stack(host)?;
        (stack.item() == paper && is_scroll(&stack)).then(|| (place, ItemResource::of(&stack)))
    })
}

fn is_scroll(stack: &ItemStack) -> bool {
    matches!(stack.components()
        .and_then(|components| components.as_nbt().get(&"minecraft:custom_data".into()))
        .and_then(|tag| tag.get("pigeonmoe:life_skills_item")),
        Some(Tag::String(value)) if value.0.iter().copied().eq("territory_scroll".encode_utf16()))
}

fn arrive(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    destination: Destination,
    landing: Point,
    payment: (ItemPlace, ItemResource),
) -> Result<(), String> {
    let target = [
        f64::from(landing[0]) + 0.5,
        f64::from(landing[1]),
        f64::from(landing[2]) + 0.5,
    ];
    if destination.origin == target {
        return Err("你已在该领地的安全落点".into());
    }
    charge_for_travel(host, payment, |host| {
        player.level(host) == Some(destination.level)
            && player.teleport_checked(host, &destination.dimension, target, None)
                == Ok(PlayerTeleportStatus::Moved)
            && player.entity().position(host) == Some(target)
    })?;
    tell(
        host,
        player,
        format!("已传送至 {}，消耗 1 个领地传送符", destination.name),
    );
    Ok(())
}

fn charge_for_travel(
    host: &mut dyn ServerHost,
    (place, scroll): (ItemPlace, ItemResource),
    move_player: impl FnOnce(&mut dyn ServerHost) -> bool,
) -> Result<(), String> {
    host.transaction(|host, transaction| {
        if place.extract(host, transaction, &scroll, 1) != 1 {
            return Err("无法取用领地传送符".into());
        }
        if !move_player(host) {
            return Err("传送没有完成".into());
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cores, model::Member};
    use mod_api::{
        prelude::{ItemsHost, PlayersHost},
        testing::FakeHost,
    };

    fn claim(owner: &Actor) -> Area {
        Area::new(
            Uuid::from_u128(10),
            "world".into(),
            Some(owner.uuid),
            owner.name.clone(),
            vec![Volume::sized([0, 64, 0], [4; 3], "starter".into()).unwrap()],
        )
    }

    #[test]
    fn only_owner_or_permitted_member_can_use_travel_even_as_operator() {
        let owner = Actor {
            uuid: Uuid::from_u128(1),
            name: "Owner".into(),
            operator: false,
        };
        let guest = Actor {
            uuid: Uuid::from_u128(2),
            name: "Guest".into(),
            operator: true,
        };
        let mut area = claim(&owner);
        let settings = Settings::default();
        assert!(can_visit(&owner, &area, &settings));
        assert!(!can_visit(&guest, &area, &settings));
        area.members.insert(
            guest.uuid.to_string(),
            Member {
                name: guest.name.clone(),
                flags: Default::default(),
            },
        );
        assert!(can_visit(&guest, &area, &settings));
        for flag in [Flag::Teleport, Flag::Move] {
            let key = guest.uuid.to_string();
            area.members
                .get_mut(&key)
                .unwrap()
                .flags
                .insert(flag, false);
            assert!(!can_visit(&guest, &area, &settings));
            area.members.get_mut(&key).unwrap().flags.remove(&flag);
        }
        area.members.clear();
        assert!(!can_visit(&guest, &area, &settings));
    }

    #[test]
    fn failed_teleport_rolls_back_only_the_real_scroll_payment() {
        let mut host = FakeHost::new();
        let paper = Identifier::parse("minecraft:paper").unwrap();
        let flint = Identifier::parse("minecraft:flint").unwrap();
        host.registries
            .resolve_items([paper.clone(), flint.clone()])
            .unwrap();
        let tagged = |id: &Identifier| {
            let mut stack = ItemStack::new(host.item_key(id).unwrap(), 2).unwrap();
            stack
                .set_components(
                    [(
                        "minecraft:custom_data".into(),
                        Tag::compound([(
                            "pigeonmoe:life_skills_item",
                            Tag::String("territory_scroll".into()),
                        )]),
                    )]
                    .into(),
                )
                .unwrap();
            stack
        };
        let scroll = tagged(&paper);
        let invalid_carrier = tagged(&flint);
        let mut named_paper = ItemStack::new(host.item_key(&paper).unwrap(), 2).unwrap();
        named_paper
            .set_components([("minecraft:custom_name".into(), cores::text("领地传送符"))].into())
            .unwrap();
        let owner = Actor {
            uuid: Uuid::from_u128(1),
            name: "Owner".into(),
            operator: false,
        };
        let slots = &mut host.join(owner.uuid.as_u128(), 1).slots;
        slots.insert(0, named_paper);
        slots.insert(1, invalid_carrier);
        let player = host.player(owner.uuid.as_u128()).unwrap();
        assert!(scroll_in_inventory(&mut host, player).is_none());
        host.players
            .get_mut(owner.uuid.as_u128())
            .unwrap()
            .slots
            .insert(2, scroll);
        let payment = scroll_in_inventory(&mut host, player).unwrap();
        assert_eq!(
            payment.0,
            ItemPlace::PlayerSlot {
                player: owner.uuid.as_u128(),
                slot: 2
            }
        );

        assert_eq!(
            charge_for_travel(&mut host, payment, |_| false),
            Err("传送没有完成".into())
        );
        for slot in [0, 1, 2] {
            assert_eq!(
                host.players.get(owner.uuid.as_u128()).unwrap().slots[&slot].count(),
                2
            );
        }
        let payment = scroll_in_inventory(&mut host, player).unwrap();
        assert_eq!(charge_for_travel(&mut host, payment, |_| true), Ok(()));
        assert_eq!(
            host.players.get(owner.uuid.as_u128()).unwrap().slots[&2].count(),
            1
        );
    }
}
