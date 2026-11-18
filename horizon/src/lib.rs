#![deny(unsafe_code)]

mod commands;
mod cores;
mod menu;
mod model;
mod service;
mod store;

#[cfg(test)]
mod tests;

use mod_api::{
    BlockSpreadEvent, BreakBlockEvent, EntityPlaceEvent, ExplosionDetonateEvent, FillBucketEvent,
    Flow, Hooks, HostSide, InteractionResult, ItemPickedUpEvent, LevelDimension, LevelHost,
    PistonPreEvent, Placement, PlayerLoggedInEvent, PlayerLoggedOutEvent, PlayerMoveEvent,
    PlayerRef, RegisterCommandsEvent, Registrar, RegistrationContext, RightClickBlockEvent,
    RunModCommandEvent, ServerHost, ServerStartedEvent, ServerStoppingEvent, ServerTickEvent, Text,
    TriState,
};
use model::{Flag, Point};
use service::{Actor, Service};
use std::sync::{Arc, Mutex, MutexGuard};
use uuid::Uuid;

#[derive(Default)]
struct Mod {
    service: Arc<Mutex<Service>>,
    registered_materials: std::sync::OnceLock<std::collections::BTreeSet<String>>,
    slimefun_loaded: std::sync::atomic::AtomicBool,
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, register: &mut dyn Registrar, context: &RegistrationContext) {
        let config = store::Store::new(data_path()).config();
        let Ok(config) = config else { return };
        let materials: std::collections::BTreeSet<_> = config
            .cores
            .values()
            .map(|core| cores::item_id(&core.item))
            .collect();
        for id in &materials {
            let key = register.item_key(id);
            register.attach_item_part(
                key,
                "purtmarsarea:core-placement",
                Arc::new(cores::CorePlacement(self.service.clone())),
            );
        }
        let _ = self.registered_materials.set(materials);
        self.slimefun_loaded.store(
            context.mods().get("slimefun").is_some(),
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    fn server_started(&self, _: &mut dyn ServerHost, event: &mut ServerStartedEvent) {
        if event.dedicated {
            let mut service = self.service();
            service.start(data_path());
            service.slimefun_loaded = self
                .slimefun_loaded
                .load(std::sync::atomic::Ordering::Relaxed);
        }
    }

    fn server_stopping(&self, _: &mut dyn ServerHost, _: &mut ServerStoppingEvent) {
        *self.service() = Service::default();
    }

    fn server_tick(&self, _: &mut dyn ServerHost, _: &mut ServerTickEvent) {
        let mut service = self.service();
        service.clicked_cores.clear();
        service.pending_placements.clear();
    }

    fn register_commands(&self, _: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        commands::register(event.commands);
    }

    fn run_mod_command(
        &self,
        host: &mut dyn ServerHost,
        event: &mut RunModCommandEvent<'_>,
    ) -> Flow {
        if !commands::belongs_to_us(&event.command.literals) {
            return Flow::Continue;
        }
        if event
            .command
            .literals
            .get(1)
            .is_some_and(|verb| verb == "menu")
        {
            if let Err(error) = menu::at_player(self, host, event.source) {
                let _ = host.commands().feedback(message(error));
            }
            return Flow::Cancel;
        }
        let reply = commands::run(self, host, event);
        for line in reply.unwrap_or_else(|error| vec![error]) {
            let _ = host.commands().feedback(message(line));
        }
        Flow::Cancel
    }

    fn menu_clicked(&self, host: &mut dyn ServerHost, event: &mut mod_api::MenuClickedEvent) {
        menu::click(self, host, event);
    }

    fn menu_closed(&self, _: &mut dyn ServerHost, event: &mut mod_api::MenuClosedEvent) {
        let mut service = self.service();
        let uuid = Uuid::from_u128(event.uuid);
        if service
            .menus
            .get(&uuid)
            .is_some_and(|menu| menu.window == event.window)
        {
            service.menus.remove(&uuid);
        }
    }

    fn player_logged_in(&self, _: &mut dyn ServerHost, event: &mut PlayerLoggedInEvent<'_>) {
        self.service()
            .remember(Uuid::from_u128(event.player), event.name);
    }

    fn player_logged_out(&self, _: &mut dyn ServerHost, event: &mut PlayerLoggedOutEvent<'_>) {
        self.service().quit(Uuid::from_u128(event.player));
    }

    fn break_block(&self, host: &mut dyn ServerHost, event: &mut BreakBlockEvent<'_>) -> Flow {
        let Some(player) = host.players().acting_player() else {
            return Flow::Continue;
        };
        let Some(dimension) = player.dimension(host) else {
            return Flow::Continue;
        };
        let dimension = &dimension.to_string();
        let core = self
            .service()
            .core(dimension, event.pos)
            .map(|area| area.id);
        if let Some(id) = core {
            let reply = self
                .break_core(host, player, id, event.pos, event.state)
                .map_or_else(|error| error, |_| "领地核心已拆除".into());
            tell(host, player, reply);
            return Flow::Cancel;
        }
        self.check(host, player, dimension, event.pos, Flag::Build)
    }

    fn entity_place(&self, host: &mut dyn ServerHost, event: &mut EntityPlaceEvent) -> Flow {
        let Some(player) = host.players().acting_player() else {
            return Flow::Continue;
        };
        let Some(dimension) = player.dimension(host) else {
            return Flow::Continue;
        };
        let dimension = &dimension.to_string();
        let uuid = Uuid::from_u128(player.uuid());
        let clicked_core = self.service().clicked_cores.remove(&uuid);
        if let Some(core_id) = clicked_core {
            if matches!(event.placement, Placement::Multi(_)) {
                return Flow::Cancel;
            }
            let (Some(actor), Some(shape)) = (
                actor(host, player),
                player
                    .level(host)
                    .and_then(|level| level.dimension_shape(host)),
            ) else {
                return Flow::Cancel;
            };
            let configured_material = self
                .service()
                .config
                .as_ref()
                .and_then(|config| config.cores.get(&core_id))
                .map(|core| cores::item_id(&core.item));
            let placed_material = event.state.block().id(host);
            if configured_material.as_deref() != Some(&placed_material) {
                tell(
                    host,
                    player,
                    "核心材质与当前配置不一致，请联系管理员重新发放",
                );
                return Flow::Cancel;
            }
            if !self
                .registered_materials
                .get()
                .is_some_and(|materials| materials.contains(&placed_material))
            {
                tell(host, player, "新核心材质需要重启服务器才能使用");
                return Flow::Cancel;
            }
            let prepared = self.service().validate_create(
                &actor,
                dimension,
                point(event.pos),
                &core_id,
                shape.min_y..shape.min_y.saturating_add(shape.height),
            );
            return match prepared {
                Ok(area) => {
                    self.service().pending_placements.insert(uuid, area);
                    Flow::Continue
                }
                Err(error) => {
                    tell(host, player, error);
                    Flow::Cancel
                }
            };
        }
        let check = |host: &mut dyn ServerHost, pos| {
            let core = self.service().core(dimension, pos).is_some();
            !core && self.check(host, player, dimension, pos, Flag::Build) == Flow::Continue
        };
        if !check(host, point(event.pos))
            || matches!(&event.placement, Placement::Multi(blocks) if blocks.iter().any(|block| !check(host, point(block.pos))))
        {
            return Flow::Cancel;
        }
        Flow::Continue
    }

    #[listener(priority = 100)]
    fn right_click_block(&self, host: HostSide, event: &mut RightClickBlockEvent<'_>) -> Flow {
        let HostSide::Server(host) = host else {
            return Flow::Continue;
        };
        let Some(player) = host.player(event.click.player) else {
            return Flow::Continue;
        };
        let dimension = dimension_name(&event.click.dimension);
        let pos = event.click.pos;
        let level = host.level(dimension);
        let block = level
            .and_then(|level| level.block(host, pos))
            .map(|block| block.id(host));
        let flag = block.as_deref().and_then(interaction_flag);
        let uuid = Uuid::from_u128(player.uuid());
        {
            let mut service = self.service();
            service.clicked_cores.remove(&uuid);
            service.pending_placements.remove(&uuid);
        }
        let under = [pos[0], pos[1].saturating_sub(1), pos[2]];
        let slimefun_loaded = self.service().slimefun_loaded;
        let slimefun = slimefun_loaded
            && matches!(
                block.as_deref(),
                Some("minecraft:crafting_table" | "minecraft:oak_fence")
            )
            && level
                .and_then(|level| level.block(host, under))
                .is_some_and(|block| block.id(host) == "minecraft:dispenser")
            && self.check(host, player, dimension, under, Flag::Container) == Flow::Cancel;
        let (core, locked) = {
            let service = self.service();
            (
                service.core(dimension, pos).is_some(),
                service.error.is_some(),
            )
        };
        if core
            || slimefun
            || locked
            || flag
                .is_some_and(|flag| self.check(host, player, dimension, pos, flag) == Flow::Cancel)
        {
            if core {
                if event.click.hand == mod_api::Hand::Main {
                    let area = self.service().core(dimension, pos).map(|area| area.id);
                    if let Some(area) = area
                        && let Err(error) = menu::open(self, host, player, area, menu::Page::Home)
                    {
                        tell(host, player, error);
                    }
                }
            }
            *event.cancellation_result = InteractionResult::Fail;
            return Flow::Cancel;
        }
        let stack = event.click.stack;
        if let Some(stack) = stack
            && cores::marker(stack).is_none()
            && modifies_block(host, &stack.item().id(host))
        {
            let adjacent =
                std::array::from_fn(|axis| pos[axis].saturating_add(event.click.hit.face[axis]));
            let target_core = self.service().core(dimension, adjacent).is_some();
            if target_core
                || self.check(host, player, dimension, pos, Flag::Build) == Flow::Cancel
                || self.check(host, player, dimension, adjacent, Flag::Build) == Flow::Cancel
            {
                *event.cancellation_result = InteractionResult::Fail;
                return Flow::Cancel;
            }
        }
        if let Some(marker) = stack.and_then(cores::marker) {
            let id = match marker {
                cores::Marker::Broken => {
                    tell(host, player, "破碎的领地核心需要修补后才能使用");
                    *event.cancellation_result = InteractionResult::Fail;
                    return Flow::Cancel;
                }
                cores::Marker::Core(id) => id,
            };
            self.service().clicked_cores.insert(uuid, id);
        }
        Flow::Continue
    }

    fn fill_bucket(&self, host: &mut dyn ServerHost, event: &mut FillBucketEvent<'_>) -> Flow {
        let Some(player) = host.players().acting_player() else {
            return Flow::Continue;
        };
        let Some(dimension) = player.dimension(host) else {
            return Flow::Continue;
        };
        if self
            .service()
            .core(&dimension.to_string(), event.pos)
            .is_some()
        {
            return Flow::Cancel;
        }
        self.check(host, player, &dimension.to_string(), event.pos, Flag::Build)
    }

    fn player_move(&self, host: &mut dyn ServerHost, event: &mut PlayerMoveEvent) -> Flow {
        let Some(player) = host.player(event.player) else {
            return Flow::Continue;
        };
        let Some(dimension) = player.dimension(host) else {
            return Flow::Continue;
        };
        let dimension = &dimension.to_string();
        let from = block_position(event.from);
        let to = block_position(event.to);
        if from == to {
            return Flow::Continue;
        }
        let result = self.check(host, player, dimension, to, Flag::Move);
        if result == Flow::Continue {
            let owner = {
                let mut service = self.service();
                if service.crosses(dimension, from, to) {
                    service
                        .at(dimension, to)
                        .or_else(|| service.at(dimension, from))
                        .map(|a| a.id)
                        .and_then(|id| service.occupation(Uuid::from_u128(event.player), id))
                } else {
                    None
                }
            };
            if let Some(owner) = owner {
                let _ = player.action_bar(
                    host,
                    Text::literal(format!("私人领地 · 已被 {owner} 占领.")),
                );
            }
        }
        result
    }

    fn bucket_empty(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::BucketEmptyEvent<'_>,
    ) -> Flow {
        let Some(player) = host.player(event.player) else {
            return Flow::Continue;
        };
        let Some(dimension) = player.dimension(host) else {
            return Flow::Continue;
        };
        let dimension = dimension.to_string();
        if self.service().core(&dimension, event.pos).is_some() {
            return Flow::Cancel;
        }
        self.check(host, player, &dimension, event.pos, Flag::Build)
    }

    fn attack_entity(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::AttackEntityEvent<'_>,
    ) -> Flow {
        let Some(player) = host.players().acting_player() else {
            return Flow::Continue;
        };
        let dimension = host.dimension();
        let pos = block_position(event.pos);
        if event.kind == "minecraft:player" {
            let service = self.service();
            return flow(service.store.is_some() && !service.natural(dimension, pos, Flag::Pvp));
        }
        let flag = if entity_inventory(event.kind) || event.kind == "minecraft:painting" {
            Flag::Build
        } else if hostile(event.kind) {
            Flag::DamageMonster
        } else {
            Flag::DamageAnimals
        };
        self.check(host, player, dimension, pos, flag)
    }

    fn entity_interact(
        &self,
        host: HostSide,
        event: &mut mod_api::EntityInteractEvent<'_>,
    ) -> Option<InteractionResult> {
        let HostSide::Server(host) = host else {
            return None;
        };
        let player = host.players().acting_player()?;
        let target = host.entity(u32::try_from(event.entity).ok()?)?;
        let pos = target.block_position(host)?;
        let flag = entity_interaction(event.kind, event.stack.map(|stack| stack.item().id(host)));
        let dimension = host.dimension();
        (self.check(host, player, dimension, pos, flag) == Flow::Cancel)
            .then_some(InteractionResult::Fail)
    }

    fn entity_interact_specific(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::EntityInteractSpecificEvent<'_>,
    ) -> Flow {
        let Some(player) = host.players().acting_player() else {
            return Flow::Continue;
        };
        let Some(target) = u32::try_from(event.entity)
            .ok()
            .and_then(|id| host.entity(id))
        else {
            return Flow::Continue;
        };
        let Some(pos) = target.block_position(host) else {
            return Flow::Continue;
        };
        let flag = entity_interaction(event.kind, event.stack.map(|stack| stack.item().id(host)));
        let dimension = host.dimension();
        self.check(host, player, dimension, pos, flag)
    }

    fn player_shear_entity(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerShearEntityEvent<'_>,
    ) -> Flow {
        let Some(target) = u32::try_from(event.entity_id)
            .ok()
            .and_then(|id| host.entity(id))
        else {
            return Flow::Continue;
        };
        self.check_entity_location(host, event.player, target, Flag::Shear)
    }

    fn fluid_place_block(
        &self,
        host: &dyn LevelHost,
        event: &mut mod_api::FluidPlaceBlockEvent<'_>,
    ) -> Flow {
        let dimension = dimension_name(&event.dimension);
        let source = point(event.liquid_pos);
        let destination = point(event.pos);
        let lava = host
            .block_state(source)
            .and_then(|state| host.state_block(state))
            .is_some_and(|block| block == "minecraft:lava");
        let flag = if lava { Flag::Lava } else { Flag::Water };
        let service = self.service();
        flow(
            service.store.is_some()
                && (service.core(dimension, destination).is_some()
                    || !service.natural(dimension, source, flag)
                    || !service.natural(dimension, destination, flag)),
        )
    }

    fn hopper_pickup_item(
        &self,
        _: &dyn LevelHost,
        event: &mut mod_api::HopperPickupItemEvent<'_>,
    ) -> Flow {
        let service = self.service();
        flow(
            service.store.is_some()
                && !service.natural(
                    dimension_name(&event.dimension),
                    point(event.pos),
                    Flag::RedstonePerform,
                ),
        )
    }

    fn hopper_inventory_search(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::HopperInventorySearchEvent,
    ) {
        let Some(target) = event.container.or_else(|| {
            event
                .entity
                .and_then(|id| host.entity(id))
                .and_then(|entity| entity.block_position(host))
        }) else {
            return;
        };
        let dim = host.dimension();
        let service = self.service();
        if service.store.is_some()
            && (service.crosses(dim, event.hopper, target)
                || !service.natural(dim, event.hopper, Flag::RedstonePerform)
                || !service.natural(dim, target, Flag::RedstonePerform))
        {
            event.container = None;
            event.entity = None;
        }
    }

    fn item_picked_up(&self, host: &mut dyn ServerHost, event: &mut ItemPickedUpEvent<'_>) {
        if self.check_entity_location(host, event.player, event.item.entity(), Flag::Pick)
            == Flow::Cancel
        {
            *event.can_pickup = TriState::False;
        }
    }

    fn explosion_detonate(&self, _: &mut dyn ServerHost, event: &mut ExplosionDetonateEvent<'_>) {
        let mut service = self.service();
        if service.store.is_some() && service.explosion(event.dimension, event.center, event.blocks)
        {
            event.blocks.clear();
        }
    }

    fn block_spread(&self, host: &dyn LevelHost, event: &mut BlockSpreadEvent<'_>) -> Flow {
        let dimension = dimension_name(&event.dimension);
        let flag = match event.kind {
            "fire" => Flag::Fire,
            "fluid" => {
                let lava = host
                    .block_state(point(event.from))
                    .and_then(|state| host.state_block(state))
                    .is_some_and(|id| id == "minecraft:lava");
                if lava { Flag::Lava } else { Flag::Water }
            }
            _ => return Flow::Continue,
        };
        let service = self.service();
        flow(
            service.store.is_some()
                && (service.core(dimension, point(event.to)).is_some()
                    || !service.natural(dimension, point(event.to), flag)
                    || (event.kind == "fluid"
                        && !service.natural(dimension, point(event.from), flag))),
        )
    }

    fn piston_pre(&self, _: &dyn LevelHost, event: &mut PistonPreEvent<'_>) -> Flow {
        let service = self.service();
        if service.store.is_none() {
            return Flow::Continue;
        }
        let dimension = dimension_name(&event.dimension);
        let origin = point(event.piston);
        let within = |pos| {
            !service.crosses(dimension, origin, pos)
                && service.natural(dimension, pos, Flag::RedstonePerform)
                && service.core(dimension, pos).is_none()
        };
        let step = |pos: Point| std::array::from_fn(|i| pos[i].saturating_add(event.direction[i]));
        let head = if event.extending {
            step(origin)
        } else {
            std::array::from_fn(|i| origin[i].saturating_sub(event.direction[i]))
        };
        flow(
            !within(origin)
                || !within(head)
                || event
                    .push
                    .iter()
                    .any(|&pos| !within(point(pos)) || !within(step(point(pos))))
                || event.destroy.iter().any(|&pos| !within(point(pos))),
        )
    }
}

impl Mod {
    fn service(&self) -> MutexGuard<'_, Service> {
        self.service
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    fn break_core(
        &self,
        host: &mut dyn ServerHost,
        player: PlayerRef,
        id: Uuid,
        pos: Point,
        old_state: mod_api::BlockState,
    ) -> Result<(), String> {
        let actor = actor(host, player).ok_or("无法读取玩家身份")?;
        let level = player.level(host).ok_or("无法读取当前维度")?;
        let drop = {
            let service = self.service();
            let config = service.ready()?;
            let area = service.index.areas.get(&id).ok_or("领地已发生变化")?;
            if !actor.manages(area) {
                return Err("只有领地主人或管理员可以管理".into());
            }
            if area.volumes.len() > 1 && area.volumes[0].core_pos == pos {
                return Err("请先拆除全部扩展核心".into());
            }
            let volume = area.core(pos).ok_or("领地核心已发生变化")?;
            let core = config.cores.get(&volume.core).ok_or("无法读取核心配置")?;
            cores::build(host, &core.drop, &volume.core, 1, true)?
        };
        if !level.remove_block(host, pos, false) {
            return Err("无法拆除领地核心".into());
        }
        let removed = self.service().remove_core(id, pos);
        if let Err(error) = removed {
            let flags = mod_api::BlockUpdateFlags::NEIGHBORS | mod_api::BlockUpdateFlags::CLIENTS;
            let _ = level.set_block(host, pos, old_state, flags);
            return Err(error);
        }
        let [x, y, z] = pos.map(|coord| f64::from(coord) + 0.5);
        if !matches!(
            level.spawn_item(host, x, y, z, drop.clone(), 10),
            Ok(Some(_))
        ) {
            player
                .give_stack(host, drop)
                .map_err(|err| format!("无法发放破碎核心：{err:?}"))?;
        }
        Ok(())
    }

    fn check(
        &self,
        host: &mut dyn ServerHost,
        player: PlayerRef,
        dimension: &str,
        pos: Point,
        flag: Flag,
    ) -> Flow {
        let Some(actor) = actor(host, player) else {
            return Flow::Continue;
        };
        let notice = {
            let mut service = self.service();
            if service.store.is_none() || service.allowed(&actor, dimension, pos, flag) {
                return Flow::Continue;
            }
            service.denial(actor.uuid, flag)
        };
        if let Some(notice) = notice {
            tell(host, player, notice);
        }
        Flow::Cancel
    }

    fn check_entity_location(
        &self,
        host: &mut dyn ServerHost,
        uuid: u128,
        entity: mod_api::EntityRef,
        flag: Flag,
    ) -> Flow {
        let Some(player) = host.player(uuid) else {
            return Flow::Continue;
        };
        let Some(dimension) = player.dimension(host) else {
            return Flow::Continue;
        };
        let Some(pos) = entity.block_position(host) else {
            return Flow::Continue;
        };
        self.check(host, player, &dimension.to_string(), pos, flag)
    }
}

fn data_path() -> std::path::PathBuf {
    std::env::var_os("PURTMARSAREA_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "plugins/PurtmarsArea".into())
}

fn actor(host: &dyn ServerHost, player: PlayerRef) -> Option<Actor> {
    Some(Actor {
        uuid: Uuid::from_u128(player.uuid()),
        name: player.name(host)?,
        admin: player
            .permission_level(host)
            .is_some_and(|level| level >= 2),
    })
}

fn block_position(pos: [f64; 3]) -> Point {
    pos.map(|coordinate| coordinate.floor() as i32)
}

fn dimension_name(dimension: &LevelDimension) -> &str {
    match dimension {
        LevelDimension::Overworld => "minecraft:overworld",
        LevelDimension::Nether => "minecraft:the_nether",
        LevelDimension::End => "minecraft:the_end",
        LevelDimension::Custom(key) => key.id(),
    }
}

fn message(line: impl Into<String>) -> Text {
    Text::literal(format!("[PurtmarsArea] {}", line.into()))
}

fn tell(host: &mut dyn ServerHost, player: PlayerRef, line: impl Into<String>) {
    let _ = player.tell(host, message(line));
}

fn flow(cancel: bool) -> Flow {
    if cancel { Flow::Cancel } else { Flow::Continue }
}

fn interaction_flag(id: &str) -> Option<Flag> {
    let name = id.strip_prefix("minecraft:")?;
    let flag = match name {
        "chest" | "trapped_chest" | "ender_chest" | "barrel" | "hopper" | "dispenser"
        | "dropper" | "furnace" | "blast_furnace" | "smoker" | "brewing_stand"
        | "chiseled_bookshelf" | "jukebox" | "composter" | "flower_pot" | "crafter" | "lectern" => {
            Flag::Container
        }
        "enchanting_table" => Flag::Enchant,
        "anvil" | "chipped_anvil" | "damaged_anvil" => Flag::Anvil,
        "cake" | "candle_cake" => Flag::Cake,
        "lever" | "repeater" | "comparator" | "daylight_detector" | "tripwire" => Flag::Redstone,
        "respawn_anchor" | "tnt" | "dragon_egg" | "bell" | "decorated_pot" | "sweet_berry_bush"
        | "cave_vines" | "cave_vines_plant" => Flag::Build,
        _ if name.ends_with("shulker_box") || name.starts_with("potted_") => Flag::Container,
        _ if name.ends_with("_door")
            || name.ends_with("_trapdoor")
            || name.ends_with("_fence_gate") =>
        {
            Flag::Door
        }
        _ if name.ends_with("_candle_cake") => Flag::Cake,
        _ if name.ends_with("_button") || name.ends_with("_pressure_plate") => Flag::Redstone,
        _ if name.ends_with("_bed") || name.ends_with("_sign") => Flag::Build,
        _ => return None,
    };
    Some(flag)
}

fn point(pos: mod_api::BlockPos) -> Point {
    [pos.x, pos.y, pos.z]
}

fn entity_inventory(kind: &str) -> bool {
    matches!(
        kind,
        "minecraft:armor_stand"
            | "minecraft:item_frame"
            | "minecraft:glow_item_frame"
            | "minecraft:minecart"
            | "minecraft:chest_minecart"
            | "minecraft:hopper_minecart"
            | "minecraft:furnace_minecart"
            | "minecraft:tnt_minecart"
            | "minecraft:command_block_minecart"
    ) || kind.ends_with("_boat")
        || kind.ends_with("_raft")
}

fn entity_interaction(kind: &str, held: Option<String>) -> Flag {
    match kind {
        "minecraft:villager" | "minecraft:wandering_trader" => Flag::Trade,
        _ if entity_inventory(kind) => Flag::Container,
        _ if held.as_deref() == Some("minecraft:lead") => Flag::Leash,
        _ => Flag::Build,
    }
}

fn hostile(kind: &str) -> bool {
    matches!(
        kind.strip_prefix("minecraft:").unwrap_or(""),
        "blaze"
            | "bogged"
            | "breeze"
            | "cave_spider"
            | "creaking"
            | "creeper"
            | "drowned"
            | "elder_guardian"
            | "ender_dragon"
            | "enderman"
            | "endermite"
            | "evoker"
            | "ghast"
            | "giant"
            | "guardian"
            | "hoglin"
            | "husk"
            | "illusioner"
            | "magma_cube"
            | "phantom"
            | "piglin"
            | "piglin_brute"
            | "pillager"
            | "ravager"
            | "shulker"
            | "silverfish"
            | "skeleton"
            | "slime"
            | "spider"
            | "stray"
            | "vex"
            | "vindicator"
            | "warden"
            | "witch"
            | "wither"
            | "wither_skeleton"
            | "zoglin"
            | "zombie"
            | "zombie_villager"
            | "zombified_piglin"
            | "parched"
    )
}

fn modifies_block(host: &dyn ServerHost, item: &str) -> bool {
    let name = item.strip_prefix("minecraft:").unwrap_or(item);
    host.block_key(item).is_some()
        || [
            "_bucket",
            "_axe",
            "_hoe",
            "_shovel",
            "_dye",
            "_spawn_egg",
            "_boat",
            "_raft",
            "minecart",
        ]
        .iter()
        .any(|suffix| name.ends_with(suffix))
        || matches!(
            name,
            "flint_and_steel"
                | "fire_charge"
                | "bone_meal"
                | "armor_stand"
                | "end_crystal"
                | "item_frame"
                | "glow_item_frame"
                | "painting"
                | "honeycomb"
                | "glass_bottle"
                | "shears"
        )
}
