#![deny(unsafe_code)]

mod combat;
mod commands;
mod cores;
mod menu;
mod model;
mod presentation;
mod service;
mod store;

#[cfg(test)]
mod tests;

use mod_api::{
    BlockSpreadEvent, BreakBlockEvent, EntityPlaceEvent, ExplosionDetonateEvent, FillBucketEvent,
    Flow, Hooks, HostBlockStates, HostSide, InteractionResult, ItemPickedUpEvent, LevelDimension,
    LevelHost, PistonPreEvent, Placement, PlayerLoggedInEvent, PlayerLoggedOutEvent,
    PlayerMoveEvent, PlayerRef, RegisterCommandsEvent, Registrar, RegistrationContext,
    RightClickBlockEvent, RunModCommandEvent, ServerHost, ServerStartedEvent, ServerStoppingEvent,
    ServerTickEvent, Text, TriState,
};
use model::{Flag, Point};
use service::{Actor, Service};
use std::sync::{Arc, Mutex, MutexGuard};
use uuid::Uuid;

#[derive(Default)]
struct Mod {
    service: Arc<Mutex<Service>>,
    boundary_particle: std::sync::OnceLock<mod_api::ParticleKey>,
    registered_materials: std::sync::OnceLock<std::collections::BTreeSet<String>>,
    slimefun_loaded: std::sync::atomic::AtomicBool,
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, register: &mut dyn Registrar, context: &RegistrationContext) {
        let _ = self
            .boundary_particle
            .set(register.particle_key("minecraft:end_rod"));
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

    fn server_started(&self, host: &mut dyn ServerHost, event: &mut ServerStartedEvent) {
        if event.dedicated {
            let mut service = self.service();
            service.start(data_path());
            if let Some(world) = host.world_dir()
                && let Err(error) = service.seed_known(&host.instance_dir(), &world)
            {
                eprintln!("[PurtmarsArea] 无法读取服务器用户缓存：{error}");
            }
            service.slimefun_loaded = self
                .slimefun_loaded
                .load(std::sync::atomic::Ordering::Relaxed);
        }
    }

    fn server_stopping(&self, _: &mut dyn ServerHost, _: &mut ServerStoppingEvent) {
        *self.service() = Service::default();
    }

    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent) {
        if matches!(event.phase, mod_api::ServerTickPhase::ServerTickEnd { .. }) {
            let notices = {
                let mut service = self.service();
                service.ticks = service.ticks.saturating_add(1);
                let oldest_transfer = service.ticks.saturating_sub(200);
                service
                    .transit_entities
                    .retain(|_, tick| *tick >= oldest_transfer);
                service.clicked_cores.clear();
                service.pending_placements.clear();
                service.denied_shots.clear();
                std::mem::take(&mut service.pending_notices)
            };
            for (id, notice) in notices {
                if let Some(player) = host.player(id.as_u128()) {
                    tell(host, player, notice);
                }
            }
            menu::poll_inputs(self, host);
            presentation::draw_boundaries(self, host);
        }
    }

    #[listener(priority = 100)]
    fn server_chat(
        &self,
        _: &mut dyn ServerHost,
        event: &mut mod_api::ServerChatEvent<'_>,
    ) -> Flow {
        flow(menu::capture_input(self, event.player, event.raw_text))
    }

    fn player_container_open(
        &self,
        _: &mut dyn ServerHost,
        event: &mut mod_api::PlayerContainerOpenEvent<'_>,
    ) {
        self.service()
            .member_inputs
            .remove(&Uuid::from_u128(event.player));
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
        let verb = event.command.literals.get(1).map(String::as_str);
        if matches!(verb, Some("menu" | "list")) {
            if let Err(error) = menu::at_player(self, host, event.source, verb == Some("list")) {
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
        let uuid = Uuid::from_u128(event.player);
        let mut service = self.service();
        if let Err(error) = service.remember(uuid, event.name) {
            eprintln!("[PurtmarsArea] 无法绑定旧领地账号：{error}");
        }
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
            let dimension = host.dimension();
            return self.protect_natural(dimension, point(event.pos), Flag::Build);
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

    fn left_click_block(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::LeftClickBlockEvent,
    ) -> Flow {
        let Some(player) = host.players().acting_player() else {
            return Flow::Continue;
        };
        let flag = if self.service().error.is_some() {
            Some(Flag::Build)
        } else {
            interaction_flag(&event.state.block().id(host))
        };
        let dimension = host.dimension();
        flag.map_or(Flow::Continue, |flag| {
            self.check(host, player, dimension, event.pos, flag)
        })
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
        let block_key = level.and_then(|level| level.block(host, pos));
        let block = block_key.map(|key| key.id(host));
        let flag = block.as_deref().and_then(interaction_flag);
        let uuid = Uuid::from_u128(player.uuid());
        {
            let mut service = self.service();
            service.clicked_cores.remove(&uuid);
            service.pending_placements.remove(&uuid);
        }
        let under = [pos[0], pos[1].saturating_sub(1), pos[2]];
        let over = [pos[0], pos[1].saturating_add(1), pos[2]];
        let slimefun_loaded = self.service().slimefun_loaded;
        let slimefun = if slimefun_loaded {
            let wooden_fence = block_key
                .is_some_and(|key| host.block_in_tag(key, "minecraft:wooden_fences") == Some(true));
            let at = |point| {
                level
                    .and_then(|level| level.block(host, point))
                    .map(|key| key.id(host))
            };
            let below_machine =
                wooden_fence || block.as_deref() == Some("minecraft:nether_brick_fence");
            let under_block = at(under);
            let above_machine = wooden_fence
                && under_block.as_deref() == Some("minecraft:cauldron")
                && at(over).as_deref() == Some("minecraft:dispenser");
            let workbench = (block.as_deref() == Some("minecraft:crafting_table"))
                .then(|| slimefun_workbench_dispenser(pos, &at))
                .flatten();
            let chamber = (block.as_deref() == Some("minecraft:cauldron"))
                .then(|| pressure_chamber(pos, &at))
                .flatten();
            (below_machine
                && under_block.as_deref() == Some("minecraft:dispenser")
                && self.check(host, player, dimension, under, Flag::Container) == Flow::Cancel)
                || (above_machine
                    && self.check(host, player, dimension, over, Flag::Container) == Flow::Cancel)
                || workbench.or(chamber).is_some_and(|dispenser| {
                    self.check(host, player, dimension, dispenser, Flag::Container) == Flow::Cancel
                })
        } else {
            false
        };
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
                        && let Err(error) = menu::open(self, host, player, menu::Page::Home(area))
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

    fn container_opened(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::ContainerOpenedEvent<'_>,
    ) -> Flow {
        let Some(player) = host.player(event.player) else {
            return Flow::Continue;
        };
        let Some(pos) = event.pos else {
            return Flow::Continue;
        };
        let dimension = host.dimension();
        let flag = match event.kind {
            "minecraft:anvil" => Flag::Anvil,
            "minecraft:enchantment" | "minecraft:enchanting" => Flag::Enchant,
            "minecraft:merchant" => Flag::Trade,
            _ => Flag::Container,
        };
        let paired = if flag == Flag::Container && event.kind != "minecraft:horse_inventory" {
            chest_partner(host, dimension, pos)
        } else {
            None
        };
        if self.check(host, player, dimension, pos, flag) == Flow::Cancel {
            return Flow::Cancel;
        }
        paired.map_or(Flow::Continue, |other| {
            self.check(host, player, dimension, other, flag)
        })
    }

    fn player_harmful_potion_target(
        &self,
        host: &dyn mod_api::Host,
        event: &mut mod_api::PlayerHarmfulPotionTargetEvent<'_>,
    ) -> Flow {
        let Some(player) = host.player(event.player) else {
            return Flow::Continue;
        };
        let pos = block_position(event.position);
        let mut service = self.service();
        if service.store.is_none() {
            return Flow::Continue;
        }
        if event.target_kind == "minecraft:player" {
            let source_dim = player
                .dimension(host)
                .map(|dimension| dimension.to_string());
            let source_pos = player.entity().block_position(host);
            return flow(combat::denies_pvp(
                &service,
                source_dim.as_deref().zip(source_pos),
                event.dimension,
                event.position,
            ));
        }
        let flag = combat::target_flag(event.target_kind, true);
        let Some(actor) = actor(host, player) else {
            return Flow::Continue;
        };
        if !service.allowed(&actor, event.dimension, pos, flag) {
            service.queue_denial(actor.uuid, flag);
            return Flow::Cancel;
        }
        Flow::Continue
    }

    fn block_dispense_pre(
        &self,
        _: &dyn mod_api::Host,
        event: &mut mod_api::BlockDispensePreEvent<'_>,
    ) -> Flow {
        let service = self.service();
        flow(
            service.store.is_some()
                && (!service.natural(event.dimension, event.pos, Flag::RedstonePerform)
                    || service.core(event.dimension, event.target).is_some()
                    || service.crosses(event.dimension, event.pos, event.target)
                    || !service.natural(event.dimension, event.target, Flag::RedstonePerform)),
        )
    }

    fn device_placement_pre(
        &self,
        host: &dyn mod_api::Host,
        event: &mut mod_api::DevicePlacementPreEvent<'_>,
    ) -> Flow {
        self.check_device_build(
            host,
            event.owner,
            event.operator,
            event.dimension,
            event.pos,
        )
    }

    fn device_break_pre(
        &self,
        host: &dyn mod_api::Host,
        event: &mut mod_api::DeviceBreakPreEvent<'_>,
    ) -> Flow {
        self.check_device_build(
            host,
            event.owner,
            event.operator,
            event.dimension,
            event.pos,
        )
    }

    fn inventory_move_item(
        &self,
        host: &dyn mod_api::Host,
        event: &mut mod_api::InventoryMoveItemEvent<'_>,
    ) -> Flow {
        let dimension = host.dimension();
        let service = self.service();
        flow(
            service.store.is_some()
                && (service.crosses(dimension, event.source, event.destination)
                    || !service.natural(dimension, event.source, Flag::RedstonePerform)
                    || !service.natural(dimension, event.destination, Flag::RedstonePerform)),
        )
    }

    fn hanging_break(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::HangingBreakEvent<'_>,
    ) -> Flow {
        let dimension = host.dimension();
        let player = event.player.and_then(|id| host.player(id));
        self.entity_action(host, player, dimension, event.pos, Flag::Build)
    }

    fn hanging_place(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::HangingPlaceEvent<'_>,
    ) -> Flow {
        let dimension = host.dimension();
        let player = host.player(event.player);
        self.entity_action(host, player, dimension, event.pos, Flag::Build)
    }

    fn player_leash_change_pre(
        &self,
        host: &dyn mod_api::Host,
        event: &mut mod_api::PlayerLeashChangePreEvent<'_>,
    ) -> Flow {
        let Some(actor) = host
            .player(event.player)
            .and_then(|player| actor(host, player))
        else {
            return Flow::Continue;
        };
        let mut service = self.service();
        if service.store.is_none()
            || service.allowed(
                &actor,
                event.dimension,
                block_position(event.position),
                Flag::Leash,
            )
        {
            return Flow::Continue;
        }
        service.queue_denial(actor.uuid, Flag::Leash);
        Flow::Cancel
    }

    fn ridden_vehicle_move_pre(
        &self,
        host: &dyn mod_api::Host,
        event: &mut mod_api::RiddenVehicleMovePreEvent<'_>,
    ) -> Flow {
        let from = block_position(event.from);
        let to = block_position(event.to);
        flow(
            self.service()
                .blocks_ridden_vehicle(event.dimension, from, to, || {
                    host.player(event.player)
                        .and_then(|player| actor(host, player))
                }),
        )
    }

    fn entity_mount(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::EntityMountEvent<'_>,
    ) -> Flow {
        if !event.mounting || !paper_vehicle(event.vehicle_kind) {
            return Flow::Continue;
        }
        let Some(player) = player_entity(host, event.entity) else {
            return Flow::Continue;
        };
        let Some(pos) = host
            .entity(event.vehicle)
            .and_then(|vehicle| vehicle.block_position(host))
        else {
            return Flow::Continue;
        };
        self.check(host, player, event.dimension, pos, Flag::Container)
    }

    fn sign_changed(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::SignChangedEvent<'_>,
    ) -> Flow {
        let Some(player) = host.player(event.player) else {
            return Flow::Continue;
        };
        self.check(host, player, event.dimension, event.pos, Flag::Build)
    }

    fn entity_change_block_pre(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::EntityChangeBlockPreEvent<'_>,
    ) -> Flow {
        let dimension = host.dimension();
        if self.service().core(dimension, event.pos).is_some() {
            return Flow::Cancel;
        }
        let player = event.player.and_then(|id| host.player(id));
        self.entity_action(host, player, dimension, event.pos, Flag::Build)
    }

    fn mob_block_change_pre(
        &self,
        _: &dyn mod_api::Host,
        event: &mut mod_api::MobBlockChangePreEvent<'_>,
    ) -> Flow {
        self.protect_natural(event.dimension, event.pos, Flag::Build)
    }

    fn sponge_absorb_pre(
        &self,
        _: &dyn LevelHost,
        event: &mut mod_api::SpongeAbsorbPreEvent,
    ) -> Flow {
        self.protect_natural(
            dimension_name(&event.dimension),
            point(event.pos),
            Flag::Water,
        )
    }

    fn falling_block_change_pre(
        &self,
        _: &dyn LevelHost,
        event: &mut mod_api::FallingBlockChangePreEvent<'_>,
    ) -> Flow {
        let dimension = dimension_name(&event.dimension);
        let origin = (!event.origin_dimension.is_empty())
            .then(|| (event.origin_dimension, point(event.origin_pos)));
        flow(
            self.service()
                .blocks_falling(dimension, point(event.pos), origin),
        )
    }

    fn block_redstone_current_pre(
        &self,
        _: &dyn LevelHost,
        event: &mut mod_api::BlockRedstoneCurrentPreEvent,
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

    fn block_trigger_entity_pre(
        &self,
        host: &dyn LevelHost,
        event: &mut mod_api::BlockTriggerEntityPreEvent<'_>,
    ) -> Flow {
        let pos = point(event.pos);
        let dimension = dimension_name(&event.dimension);
        let block = host
            .block_state(pos)
            .and_then(|state| host.state_block(state));
        if block == Some("minecraft:turtle_egg") {
            return flow(
                self.service()
                    .blocks_egg_trample(dimension, pos, event.player.is_some()),
            );
        }
        let flag = if block == Some("minecraft:big_dripleaf") {
            Flag::Build
        } else {
            Flag::Redstone
        };
        let player = event.player.map(|uuid| Actor {
            uuid: Uuid::from_u128(uuid),
            name: event.name.to_owned(),
            operator: event.permission_level > 0,
        });
        flow(
            self.service()
                .blocks_physical_trigger(dimension, pos, player.as_ref(), flag),
        )
    }

    fn block_fade(&self, _: &dyn LevelHost, event: &mut mod_api::BlockFadeEvent) -> Flow {
        flow(
            self.service()
                .core(dimension_name(&event.dimension), point(event.pos))
                .is_some(),
        )
    }

    fn block_form(&self, _: &dyn LevelHost, event: &mut mod_api::BlockFormEvent) -> Flow {
        flow(
            self.service()
                .core(dimension_name(&event.dimension), point(event.pos))
                .is_some(),
        )
    }

    fn block_burn(&self, _: &dyn LevelHost, event: &mut mod_api::BlockBurnEvent) -> Flow {
        self.protect_natural(
            dimension_name(&event.dimension),
            point(event.pos),
            Flag::Fire,
        )
    }

    fn block_ignite(&self, _: &dyn LevelHost, event: &mut mod_api::BlockIgniteEvent<'_>) -> Flow {
        let dimension = dimension_name(&event.dimension);
        let pos = point(event.pos);
        if event.player.is_some() {
            // The player's use-on permission was checked against both blocks in right_click_block.
            flow(self.service().core(dimension, pos).is_some())
        } else {
            self.protect_natural(dimension, pos, Flag::Fire)
        }
    }

    fn tnt_primed(&self, host: &mut dyn ServerHost, event: &mut mod_api::TntPrimedEvent) -> Flow {
        self.protect_natural(host.dimension(), event.pos, Flag::Explosion)
    }

    fn crop_grow_pre(
        &self,
        _: &mut dyn mod_api::LevelRandomHost,
        event: &mut mod_api::CropGrowPreEvent<'_>,
    ) {
        if self
            .service()
            .core(dimension_name(&event.dimension), point(event.pos))
            .is_some()
        {
            *event.result = TriState::False;
        }
    }

    fn block_physics_pre(
        &self,
        _: &dyn LevelHost,
        event: &mut mod_api::BlockPhysicsPreEvent,
    ) -> Flow {
        flow(
            self.service()
                .core(dimension_name(&event.dimension), point(event.pos))
                .is_some(),
        )
    }

    fn block_growth_candidate_pre(
        &self,
        _: &dyn LevelHost,
        event: &mut mod_api::BlockGrowthCandidatePreEvent<'_>,
    ) -> Flow {
        let player = event.player.map(|uuid| Actor {
            uuid: Uuid::from_u128(uuid),
            name: event.name.to_owned(),
            operator: event.permission_level > 0,
        });
        flow(self.service().blocks_growth(
            dimension_name(&event.dimension),
            point(event.origin),
            point(event.pos),
            event.structure,
            event.fertilized,
            player.as_ref(),
        ))
    }

    fn entity_travel_to_dimension(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::EntityTravelToDimensionEvent<'_>,
    ) -> Flow {
        // Fresh insertion at a portal is travel, not CreatureSpawnEvent.
        if let Some(uuid) = event.entity.uuid(host) {
            let mut service = self.service();
            let tick = service.ticks;
            service.transit_entities.insert(uuid, tick);
        }
        Flow::Continue
    }

    fn entity_join_level(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::EntityJoinLevelEvent<'_>,
    ) -> Flow {
        if event.loaded_from_disk {
            return Flow::Continue;
        }
        let Some(entity) = host.entity(event.entity) else {
            return Flow::Continue;
        };
        let Some(flag) = creature_spawn_flag(event.kind, host.living(entity).is_some()) else {
            return Flow::Continue;
        };
        let uuid = entity.uuid(host);
        let mut service = self.service();
        if service.store.is_none()
            || uuid.is_some_and(|id| service.transit_entities.remove(&id).is_some())
        {
            return Flow::Continue;
        }
        flow(!service.natural(event.dimension, block_position(event.pos), flag))
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
            self.transition(host, player, dimension, from, dimension, to);
        }
        result
    }

    fn teleport_command(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::TeleportCommandEvent<'_>,
    ) -> Flow {
        let Some(player) = player_entity(host, event.entity) else {
            return Flow::Continue;
        };
        let Some(origin) = player.dimension(host) else {
            return Flow::Continue;
        };
        self.teleport(
            host,
            player,
            (&origin.to_string(), block_position(event.from)),
            (event.dimension, block_position(*event.to)),
        )
    }

    fn player_teleport_pre(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerTeleportPreEvent<'_>,
    ) -> Flow {
        let Some(player) = host.player(event.player) else {
            return Flow::Continue;
        };
        self.teleport(
            host,
            player,
            (event.from_level, block_position(event.from)),
            (event.to_level, block_position(event.to)),
        )
    }

    fn player_changed_dimension(
        &self,
        _: &mut dyn ServerHost,
        event: &mut mod_api::PlayerChangedDimensionEvent<'_>,
    ) {
        self.service()
            .member_inputs
            .remove(&Uuid::from_u128(event.player));
    }

    fn player_drop_pre(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerDropPreEvent<'_>,
    ) -> Flow {
        let Some(player) = host.player(event.player) else {
            return Flow::Continue;
        };
        self.check(
            host,
            player,
            event.dimension,
            block_position(event.position),
            Flag::Drop,
        )
    }

    fn player_pickup_arrow_pre(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerPickupArrowPreEvent<'_>,
    ) -> Flow {
        let Some(player) = host.player(event.player) else {
            return Flow::Continue;
        };
        self.check(
            host,
            player,
            event.dimension,
            block_position(event.position),
            Flag::Pick,
        )
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
            let source_pos = player.entity().block_position(host);
            return flow(combat::denies_pvp(
                &self.service(),
                source_pos.map(|source| (dimension, source)),
                dimension,
                event.pos,
            ));
        }
        let living = host
            .entity(event.entity as u32)
            .and_then(|entity| host.living(entity))
            .is_some();
        self.check(
            host,
            player,
            dimension,
            pos,
            combat::target_flag(event.kind, living),
        )
    }

    fn hears_projectile_impacts(
        &self,
        _: &dyn mod_api::Host,
        event: &mut mod_api::HearsProjectileImpactsEvent,
    ) {
        let service = self.service();
        event.hears |= service.store.is_some()
            && (service.error.is_some()
                || !service.index.areas.is_empty()
                || service
                    .config
                    .as_ref()
                    .is_some_and(|config| !config.settings.claim_world.is_empty()));
    }

    fn projectile_impact(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::ProjectileImpactEvent<'_>,
    ) -> Flow {
        let Some(target_id) = event.entity else {
            return Flow::Continue;
        };
        let Some(shot) =
            combat::Shot::at_impact(event.kind, event.projectile, event.owner, target_id)
        else {
            return Flow::Continue;
        };
        let Some(target) = host.entity(target_id) else {
            return Flow::Continue;
        };
        let Some(position) = target.position(host) else {
            return Flow::Continue;
        };
        let owner = (event.owner != 0)
            .then(|| player_entity(host, event.owner))
            .flatten();
        let living = host.living(target).is_some();
        let mut service = self.service();
        if service.store.is_none() {
            return Flow::Continue;
        }
        let denied = if event.entity_kind == "minecraft:player" {
            owner.is_some_and(|player| {
                let source = player.dimension(host).map(|dim| dim.to_string());
                combat::denies_pvp(
                    &service,
                    source.as_deref().zip(player.entity().block_position(host)),
                    event.dimension,
                    position,
                )
            })
        } else {
            let flag = combat::target_flag(event.entity_kind, living);
            let target = block_position(position);
            match owner.and_then(|player| actor(host, player)) {
                Some(owner) => !service.allowed(&owner, event.dimension, target, flag),
                None => !living && !service.natural(event.dimension, target, flag),
            }
        };
        if denied {
            service.denied_shots.insert(shot);
        } else {
            service.denied_shots.remove(&shot);
        }
        Flow::Continue
    }

    fn entity_invulnerability_check(
        &self,
        level: mod_api::LevelSide<'_>,
        event: &mut mod_api::EntityInvulnerabilityCheckEvent<'_>,
    ) {
        if matches!(level, mod_api::LevelSide::Server(_))
            && !event.invulnerable
            && let Some(shot) = combat::Shot::at_damage(event.entity, event.source)
        {
            event.invulnerable = self.service().denied_shots.contains(&shot);
        }
    }

    fn entity_damage_target_pre(
        &self,
        _: &mut dyn mod_api::LevelRandomHost,
        event: &mut mod_api::EntityDamageTargetPreEvent<'_>,
    ) -> Flow {
        flow(combat::denies_entity_damage(
            &mut self.service(),
            event.dimension,
            event.position,
            event.target_kind,
            event.alive_kind,
            event.player,
        ))
    }

    fn vehicle_damage_pre(
        &self,
        _: &dyn mod_api::Host,
        event: &mut mod_api::VehicleDamagePreEvent<'_>,
    ) -> Flow {
        flow(combat::denies_entity_damage(
            &mut self.service(),
            event.dimension,
            event.position,
            event.kind,
            false,
            event.player,
        ))
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

    fn hopper_pickup_item_pre(
        &self,
        _: &dyn LevelHost,
        event: &mut mod_api::HopperPickupItemPreEvent<'_>,
    ) -> Flow {
        let dimension = dimension_name(&event.dimension);
        let pos = point(event.pos);
        let source = block_position(event.source);
        let service = self.service();
        flow(
            service.store.is_some()
                && (service.crosses(dimension, source, pos)
                    || !service.natural(dimension, pos, Flag::RedstonePerform)),
        )
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
        let destination = point(event.to);
        let flag = match event.kind {
            "fire" => return self.protect_natural(dimension, destination, Flag::Fire),
            "growth" => return self.protect_natural(dimension, destination, Flag::Build),
            "dragon_egg" => Flag::Water,
            "fluid" => {
                let lava = host
                    .block_state(point(event.from))
                    .and_then(|state| host.state_block(state))
                    .is_some_and(|id| id == "minecraft:lava");
                if lava { Flag::Lava } else { Flag::Water }
            }
            _ => return Flow::Continue,
        };
        flow(
            self.service()
                .blocks_flow(dimension, point(event.from), destination, flag),
        )
    }

    fn piston_pre(&self, _: &dyn LevelHost, event: &mut PistonPreEvent<'_>) -> Flow {
        flow(
            self.service().blocks_piston(
                dimension_name(&event.dimension),
                point(event.piston),
                piston_protection_direction(
                    event.direction,
                    event.extending,
                    !event.push.is_empty() || !event.destroy.is_empty(),
                ),
                event
                    .push
                    .iter()
                    .chain(event.destroy.iter())
                    .copied()
                    .map(point),
            ),
        )
    }
}

impl Mod {
    fn check_device_build(
        &self,
        host: &dyn mod_api::Host,
        owner: u128,
        operator: bool,
        dimension: &str,
        pos: Point,
    ) -> Flow {
        let online_name = host.player(owner).and_then(|player| player.name(host));
        flow(!self.service().allows_device_build(
            Uuid::from_u128(owner),
            online_name.as_deref(),
            operator,
            dimension,
            pos,
        ))
    }

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
            if !actor.manages(&config.settings, area) {
                return Err("只有领地主人或管理员可以管理".into());
            }
            if area.volumes.len() > 1 && area.volumes[0].core_pos == pos {
                return Err("请先拆除全部扩展核心".into());
            }
            let volume = area.core(pos).ok_or("领地核心已发生变化")?;
            let core = config.cores.get(&volume.core).ok_or("无法读取核心配置")?;
            cores::build(host, &core.drop_item(), &volume.core, 1, true)?
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

    fn teleport(
        &self,
        host: &mut dyn ServerHost,
        player: PlayerRef,
        from: (&str, Point),
        to: (&str, Point),
    ) -> Flow {
        if self.check(host, player, from.0, from.1, Flag::Teleport) == Flow::Cancel
            || self.check(host, player, to.0, to.1, Flag::Teleport) == Flow::Cancel
            || self.check(host, player, to.0, to.1, Flag::Move) == Flow::Cancel
        {
            return Flow::Cancel;
        }
        self.transition(host, player, from.0, from.1, to.0, to.1);
        Flow::Continue
    }

    fn transition(
        &self,
        host: &mut dyn ServerHost,
        player: PlayerRef,
        from_dim: &str,
        from: Point,
        to_dim: &str,
        to: Point,
    ) {
        let uuid = Uuid::from_u128(player.uuid());
        let owner = {
            let mut service = self.service();
            let from = service.at(from_dim, from).map(|area| area.id);
            let to = service.at(to_dim, to).map(|area| area.id);
            if from_dim != to_dim || from != to {
                service.member_inputs.remove(&uuid);
            }
            if from != to {
                to.or(from).and_then(|id| service.occupation(uuid, id))
            } else {
                None
            }
        };
        if let Some(owner) = owner {
            presentation::occupation(host, player, &owner);
        }
    }

    fn protect_natural(&self, dimension: &str, pos: Point, flag: Flag) -> Flow {
        let service = self.service();
        flow(
            service.store.is_some()
                && (service.core(dimension, pos).is_some()
                    || !service.natural(dimension, pos, flag)),
        )
    }

    fn entity_action(
        &self,
        host: &mut dyn ServerHost,
        player: Option<PlayerRef>,
        dimension: &str,
        pos: Point,
        flag: Flag,
    ) -> Flow {
        if let Some(player) = player {
            return self.check(host, player, dimension, pos, flag);
        }
        let service = self.service();
        flow(service.store.is_some() && !service.natural(dimension, pos, flag))
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

fn actor(host: &dyn mod_api::Host, player: PlayerRef) -> Option<Actor> {
    Some(Actor {
        uuid: Uuid::from_u128(player.uuid()),
        name: player.name(host)?,
        operator: player.permission_level(host).is_some_and(|level| level > 0),
    })
}

fn player_entity(host: &dyn ServerHost, id: u32) -> Option<PlayerRef> {
    host.players()
        .acting_player()
        .filter(|player| player.entity().id() == id)
        .or_else(|| {
            host.players()
                .server_players()
                .into_iter()
                .find(|player| player.entity().id() == id)
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
        "chest" | "trapped_chest" | "barrel" | "hopper" | "dispenser" | "dropper" | "furnace"
        | "blast_furnace" | "smoker" | "brewing_stand" | "chiseled_bookshelf" | "jukebox"
        | "composter" | "flower_pot" | "crafter" | "lectern" | "decorated_pot" => Flag::Container,
        "enchanting_table" => Flag::Enchant,
        "anvil" | "chipped_anvil" | "damaged_anvil" => Flag::Anvil,
        "cake" | "candle_cake" => Flag::Cake,
        "lever" | "repeater" | "comparator" | "daylight_detector" | "tripwire" => Flag::Redstone,
        "respawn_anchor" | "tnt" | "dragon_egg" | "bell" | "sweet_berry_bush" | "cave_vines"
        | "cave_vines_plant" => Flag::Build,
        _ if name.ends_with("shulker_box")
            || name.ends_with("_shelf")
            || name.starts_with("potted_") =>
        {
            Flag::Container
        }
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

fn piston_protection_direction(direction: Point, extending: bool, moves_blocks: bool) -> Point {
    // Paper's empty retract event supplies the piston's facing. A resolved
    // sticky pull supplies the movement direction. The Paper plugin inverts
    // both when checking the neighboring positions.
    if !extending && moves_blocks {
        return direction.map(i32::saturating_neg);
    }
    direction
}

fn paper_vehicle(kind: &str) -> bool {
    let Some(name) = kind.strip_prefix("minecraft:") else {
        return false;
    };
    matches!(
        name,
        "boat"
            | "minecart"
            | "pig"
            | "strider"
            | "horse"
            | "skeleton_horse"
            | "zombie_horse"
            | "donkey"
            | "mule"
            | "camel"
            | "camel_husk"
            | "llama"
            | "trader_llama"
            | "nautilus"
            | "zombie_nautilus"
            | "happy_ghast"
    ) || name.ends_with("_minecart")
        || name.ends_with("_boat")
        || name.ends_with("_raft")
}

fn entity_interaction(kind: &str, held: Option<String>) -> Flag {
    match kind {
        "minecraft:villager" | "minecraft:wandering_trader" => Flag::Trade,
        "minecraft:armor_stand"
        | "minecraft:item_frame"
        | "minecraft:glow_item_frame"
        | "minecraft:allay"
        | "minecraft:piglin"
        | "minecraft:pillager"
        | "minecraft:player" => Flag::Container,
        _ if paper_vehicle(kind) => Flag::Container,
        _ if held.as_deref() == Some("minecraft:lead") => Flag::Leash,
        _ => Flag::Build,
    }
}

fn slimefun_workbench_dispenser(
    table: Point,
    at: impl Fn(Point) -> Option<String>,
) -> Option<Point> {
    let [x, y, z] = table;
    let mut first_dispenser = None;
    // Slimefun recognizes any rotated Magic Workbench and looks up its
    // inventory by the first horizontal dispenser in this order.
    for (dx, dz) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
        let dispenser = [x.saturating_add(dx), y, z.saturating_add(dz)];
        if at(dispenser).as_deref() != Some("minecraft:dispenser") {
            continue;
        }
        let inventory = *first_dispenser.get_or_insert(dispenser);
        let shelf = [x.saturating_sub(dx), y, z.saturating_sub(dz)];
        if at(shelf).as_deref() == Some("minecraft:bookshelf") {
            return Some(inventory);
        }
    }
    let under = [x, y.saturating_sub(1), z];
    (at(under).as_deref() == Some("minecraft:dispenser")).then_some(under)
}

fn pressure_chamber(cauldron: Point, at: impl Fn(Point) -> Option<String>) -> Option<Point> {
    let [x, y, z] = cauldron;
    let middle = [x, y.saturating_add(1), z];
    let top = [x, y.saturating_add(2), z];
    if at(middle).as_deref() != Some("minecraft:glass")
        || at(top).as_deref() != Some("minecraft:dispenser")
    {
        return None;
    }
    [0, 2]
        .into_iter()
        .any(|axis| {
            [
                (cauldron, "minecraft:piston"),
                (middle, "minecraft:piston"),
                (top, "minecraft:smooth_stone_slab"),
            ]
            .into_iter()
            .all(|(pos, material)| {
                [-1, 1].into_iter().all(|side| {
                    let mut side_pos = pos;
                    side_pos[axis] = side_pos[axis].saturating_add(side);
                    at(side_pos).as_deref() == Some(material)
                })
            })
        })
        .then_some(top)
}

fn creature_spawn_flag(kind: &str, living: bool) -> Option<Flag> {
    // Paper fires CreatureSpawnEvent for every non-player LivingEntity,
    // including armor stands whose vanilla spawn category is Misc.
    if !living || kind == "minecraft:player" {
        return None;
    }
    Some(if hostile(kind) {
        Flag::SpawnMonster
    } else {
        Flag::SpawnAnimals
    })
}

fn hostile(kind: &str) -> bool {
    // Paper's Monster interface is narrower than its Enemy interface.
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
            | "enderman"
            | "endermite"
            | "evoker"
            | "giant"
            | "guardian"
            | "husk"
            | "illusioner"
            | "piglin"
            | "piglin_brute"
            | "pillager"
            | "ravager"
            | "silverfish"
            | "skeleton"
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

fn chest_partner(host: &dyn ServerHost, dimension: &str, pos: Point) -> Option<Point> {
    let level = host.level(dimension)?;
    let state = level.block_state(host, pos)?;
    let block = state.block().id(host);
    if !block.ends_with("chest") || block.ends_with("ender_chest") {
        return None;
    }
    let metadata = host.block_states();
    let sign = match state.property(&metadata, "type").ok().flatten()? {
        "left" => 1,
        "right" => -1,
        _ => return None,
    };
    let [dx, dz] = match state.property(&metadata, "facing").ok().flatten()? {
        "north" => [sign, 0],
        "east" => [0, sign],
        "south" => [-sign, 0],
        "west" => [0, -sign],
        _ => return None,
    };
    let partner = [pos[0] + dx, pos[1], pos[2] + dz];
    (level.block_state(host, partner)?.block() == state.block()).then_some(partner)
}
