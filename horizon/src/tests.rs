use crate::model::{Area, AreaIndex, ExpansionDirection, Flag, Member, Volume};
use crate::service::{Actor, Service};
use crate::store::{Store, parse};
use std::{collections::BTreeMap, fs};
use uuid::Uuid;

const OWNER: Uuid = Uuid::from_u128(1);
const GUEST: Uuid = Uuid::from_u128(2);

fn area(id: u128, pos: [i32; 3], edge: i32) -> Area {
    Area::new(
        Uuid::from_u128(id),
        "world".into(),
        Some(OWNER),
        "Owner".into(),
        vec![Volume::sized(pos, [edge; 3], "0".into()).unwrap()],
    )
}

#[test]
fn armor_stand_equipment_checks_the_stands_latest_position_and_container_permission() {
    use mod_api::{
        Backend, BackendMut, BackendMutRef, BackendRef, EntityRef, Host, Identifier, ModEntry,
        PlayerRef, ServerHost, Text, TextFormError,
        prelude::{
            EntitiesBackend, EntitiesHost, PlayersBackend, PlayersBackendMut, PlayersClientBackend,
            PlayersHost,
        },
        testing::FakeHost,
    };

    const STAND: u32 = 31;
    struct StandHost {
        fake: FakeHost,
        position: [f64; 3],
    }

    impl Host for StandHost {
        fn backend(&self) -> Option<BackendRef<'_>> {
            Some(mod_api::__private::parts_installation::backend_ref(self))
        }
    }
    impl EntitiesHost for StandHost {
        fn entity(&self, id: u32) -> Option<EntityRef> {
            self.fake.entity(id)
        }
    }
    impl PlayersHost for StandHost {
        fn player(&self, uuid: u128) -> Option<PlayerRef> {
            self.fake.player(uuid)
        }
    }
    mod_api::impl_host_areas!(StandHost, except(entities, players));

    impl Backend for StandHost {}
    impl EntitiesBackend for StandHost {
        fn entity_block_position(&self, entity: EntityRef) -> Option<[i32; 3]> {
            (entity.id() == STAND).then(|| crate::block_position(self.position))
        }
    }
    impl PlayersBackend for StandHost {
        fn player_name(&self, player: PlayerRef) -> Option<String> {
            self.fake.player_name(player)
        }
        fn player_dimension(&self, player: PlayerRef) -> Option<Identifier> {
            self.fake.player_dimension(player)
        }
        fn player_permission_level(&self, _: PlayerRef) -> Option<u8> {
            Some(0)
        }
    }
    impl PlayersClientBackend for StandHost {}
    mod_api::impl_backend_areas!(StandHost, except(entities, players));

    impl ServerHost for StandHost {
        fn backend_mut(&mut self) -> Option<BackendMutRef<'_>> {
            Some(mod_api::__private::parts_installation::backend_mut_ref(self))
        }
    }
    mod_api::impl_server_host_areas!(StandHost);
    impl BackendMut for StandHost {}
    impl PlayersBackendMut for StandHost {
        fn player_tell(&mut self, _: PlayerRef, _: Text) -> Result<bool, TextFormError> {
            Ok(true)
        }
    }
    mod_api::impl_backend_mut_areas!(StandHost, except(players));

    mod_api::__private::initialize().unwrap();
    let _phase = mod_api::__private::PhaseOwner::scope(mod_api::__private::ModPhase::World);
    let dir = tempfile::tempdir().unwrap();
    let plugin = crate::Mod::default();
    {
        let mut service = plugin.service();
        service.start(dir.path().into());
        let mut claim = area(1, [0, 64, 0], 8);
        claim.flags.insert(Flag::Build, true);
        claim.flags.insert(Flag::Container, false);
        service.index.add(claim).unwrap();
    }
    let mut host = StandHost {
        fake: FakeHost::new(),
        position: [5.0, 64.0, 0.0],
    };
    host.fake.join(OWNER.as_u128(), 11).name = "Owner".into();
    host.fake.join(GUEST.as_u128(), 12).name = "Guest".into();
    host.fake.level.entity(STAND, "minecraft:armor_stand");
    let swap = |host: &mut StandHost, actor: Uuid, stand: u32| {
        let mut cancel = false;
        mod_api::dispatch_armor_stand_manipulate(
            host,
            actor.as_u128(),
            "Player",
            stand as i32,
            "head",
            &mut cancel,
        );
        cancel
    };
    let mods = [ModEntry::new("purtmarsarea", Box::new(plugin))].into();
    mod_api::testing::with_dispatch(mods, || {
        assert!(!swap(&mut host, GUEST, STAND));
        host.position = [-3.1, 64.0, 0.0];
        assert!(swap(&mut host, GUEST, STAND));
        assert!(!swap(&mut host, OWNER, STAND));
        assert!(!swap(&mut host, GUEST, STAND + 1));
        host.position = [-4.1, 64.0, 0.0];
        assert!(!swap(&mut host, GUEST, STAND));
    });
}

#[test]
fn exact_bounds_and_negative_chunk_lookup() {
    let a = area(1, [-16, 64, -16], 8);
    assert_eq!(a.volumes[0].min, [-20, 60, -20]);
    assert_eq!(a.volumes[0].max, [-13, 67, -13]);
    let mut index = AreaIndex::default();
    index.add(a).unwrap();
    assert!(index.at("world", [-20, 60, -20]).is_some());
    assert!(index.at("world", [-13, 67, -13]).is_some());
    assert!(index.at("world", [-12, 67, -13]).is_none());
    assert!(index.at("world_nether", [-13, 67, -13]).is_none());
}

#[test]
fn ruler_fallthrough_consumes_only_the_matching_recent_click() {
    let mut service = Service::default();
    let world = "minecraft:overworld";
    let nether = "minecraft:the_nether";
    service.record_ruler_block_click(OWNER, world, 1000);
    assert!(!service.take_ruler_fallthrough(GUEST, world, 1020));
    assert!(service.take_ruler_fallthrough(OWNER, world, 1100));
    assert!(!service.take_ruler_fallthrough(OWNER, world, 1101));

    service.record_ruler_block_click(OWNER, world, 2000);
    assert!(!service.take_ruler_fallthrough(OWNER, nether, 2020));
    service.record_ruler_block_click(OWNER, world, 3000);
    assert!(!service.take_ruler_fallthrough(OWNER, world, 3151));
    service.record_ruler_block_click(OWNER, world, 4000);
    service.quit(OWNER);
    assert!(!service.take_ruler_fallthrough(OWNER, world, 4010));
}

#[test]
fn large_claim_can_be_inside_the_volume_but_out_of_particle_range() {
    let volume = Volume::new([0, 64, 0], [-64, 0, -64], [64, 132, 64], "0".into()).unwrap();
    let settings = crate::store::Particle::default();
    let points = |viewer| crate::presentation::boundary_points([&volume].into_iter(), viewer, &settings);
    assert!(points([0.5, 64.0, 0.5]).is_empty());
    assert!(!points([-63.5, 64.0, -63.5]).is_empty());
}

#[test]
fn overlap_rules_cover_the_whole_cuboid() {
    let mut index = AreaIndex::default();
    index.add(area(1, [0, 64, 0], 64)).unwrap();
    assert!(index.add(area(2, [0, 64, 0], 8)).is_err());
    assert!(index.add(area(2, [35, 64, 0], 8)).is_err());
    index.add(area(2, [36, 64, 0], 8)).unwrap();
    index.add(area(3, [0, 100, 0], 8)).unwrap();
}

#[test]
fn non_lava_flow_checks_both_endpoints_and_the_destination_core() {
    let dir = tempfile::tempdir().unwrap();
    let mut service = Service::default();
    service.start(dir.path().into());
    let mut claim = area(1, [0, 64, 0], 8);
    claim.flags.insert(Flag::Build, true);
    claim.flags.insert(Flag::Lava, true);
    claim.flags.insert(Flag::Water, false);
    service.index.add(claim.clone()).unwrap();

    let dim = "minecraft:overworld";
    let inside = [1, 64, 0];
    let outside = [20, 64, 0];
    let core = [0, 64, 0];
    assert!(service.blocks_flow(dim, inside, outside, Flag::Water));
    assert!(service.blocks_flow(dim, outside, inside, Flag::Water));
    assert!(!service.blocks_flow(dim, outside, [21, 64, 0], Flag::Water));
    assert!(!service.blocks_flow(dim, outside, inside, Flag::Lava));

    service.index = AreaIndex::default();
    claim.flags.insert(Flag::Water, true);
    service.index.add(claim).unwrap();
    assert!(!service.blocks_flow(dim, inside, outside, Flag::Water));
    assert!(!service.blocks_flow(dim, outside, inside, Flag::Water));
    assert!(service.blocks_flow(dim, outside, core, Flag::Water));
}

#[test]
fn falling_rules_distinguish_the_original_claim_from_external_or_unknown_origins() {
    let dir = tempfile::tempdir().unwrap();
    let mut service = Service::default();
    service.start(dir.path().into());
    let mut claim = area(1, [0, 64, 0], 8);
    service.index.add(claim.clone()).unwrap();
    let dim = "minecraft:overworld";
    let inside = [0, 63, 0];
    let outside = [0, 68, 0];
    assert!(service.blocks_falling(dim, [0, 64, 0], Some((dim, [0, 64, 0]))));
    assert!(!service.blocks_falling(dim, inside, Some((dim, [0, 66, 0]))));
    assert!(service.blocks_falling(dim, inside, Some((dim, outside))));
    assert!(service.blocks_falling(dim, inside, Some(("minecraft:the_nether", inside))));
    assert!(service.blocks_falling(dim, inside, None));
    assert!(!service.blocks_falling(dim, outside, Some((dim, inside))));

    service.index = AreaIndex::default();
    claim.flags.insert(Flag::Falling, true);
    service.index.add(claim).unwrap();
    assert!(!service.blocks_falling(dim, inside, Some((dim, outside))));
    assert!(service.blocks_falling(dim, [0, 64, 0], None));
}

#[test]
fn devices_use_saved_owner_and_member_uuids_without_a_live_player_name() {
    let dir = tempfile::tempdir().unwrap();
    let mut service = Service::default();
    service.start(dir.path().into());
    let mut claim = area(1, [0, 64, 0], 8);
    claim.flags.insert(Flag::Build, false);
    claim.members.insert(
        GUEST.to_string(),
        Member {
            name: "FormerName".into(),
            flags: [(Flag::Build, true)].into(),
        },
    );
    service.index.add(claim).unwrap();
    let dim = "minecraft:overworld";
    let stranger = Uuid::from_u128(3);
    assert!(service.allows_device_build(OWNER, None, false, dim, [0, 63, 0]));
    assert!(service.allows_device_build(GUEST, None, false, dim, [0, 63, 0]));
    assert!(!service.allows_device_build(stranger, None, false, dim, [0, 63, 0]));
    assert!(service.allows_device_build(stranger, None, true, dim, [0, 63, 0]));
    assert!(!service.allows_device_build(OWNER, None, true, dim, [0, 64, 0]));
}

#[test]
fn projectile_veto_matches_only_the_original_damage_attempt() {
    use crate::combat::{Shot, target_flag};
    use mod_api::entity::health::HurtSource;

    let mut service = Service::default();
    let shot = Shot::at_impact("minecraft:spectral_arrow", 21, 11, 12).unwrap();
    service.denied_shots.insert(shot);
    let another = Shot::at_impact("minecraft:trident", 31, 11, 12).unwrap();
    service.denied_shots.insert(another);
    let other_shooter = Shot::at_impact("minecraft:arrow", 41, 13, 12).unwrap();
    service.denied_shots.insert(other_shooter);
    let impact = HurtSource {
        damage_type: "minecraft:arrow",
        causing: 11,
        direct: 21,
        position: None,
    };
    let denied = |target, source| {
        Shot::at_damage(target, &source).is_some_and(|hit| service.denied_shots.contains(&hit))
    };
    assert!(denied(12, impact));
    assert!(denied(
        12,
        HurtSource {
            damage_type: "minecraft:trident",
            direct: 31,
            ..impact
        }
    ));
    assert!(denied(
        12,
        HurtSource {
            causing: 13,
            direct: 41,
            ..impact
        }
    ));
    assert!(!denied(
        12,
        HurtSource {
            causing: 11,
            direct: 41,
            ..impact
        }
    ));
    assert!(!denied(13, impact));
    assert!(!denied(
        12,
        HurtSource {
            causing: 0,
            ..impact
        }
    ));
    assert!(!denied(
        12,
        HurtSource {
            direct: 22,
            ..impact
        }
    ));
    assert!(!denied(
        12,
        HurtSource {
            damage_type: "minecraft:trident",
            ..impact
        }
    ));
    assert!(!denied(
        12,
        HurtSource {
            damage_type: "minecraft:magic",
            ..impact
        }
    ));
    assert_eq!(target_flag("minecraft:end_crystal", false), Flag::Build);
    assert_eq!(target_flag("minecraft:oak_boat", false), Flag::Build);
    assert_eq!(
        target_flag("minecraft:armor_stand", true),
        Flag::DamageAnimals
    );
    assert_eq!(target_flag("minecraft:zombie", true), Flag::DamageMonster);
    assert_eq!(target_flag("minecraft:slime", true), Flag::DamageAnimals);
    assert_eq!(
        target_flag("minecraft:ender_dragon", true),
        Flag::DamageAnimals
    );
}

#[test]
fn growth_filters_the_structure_boundary_and_then_the_fertilizer_actor() {
    let dir = tempfile::tempdir().unwrap();
    let mut service = Service::default();
    service.start(dir.path().into());
    service.index.add(area(1, [0, 64, 0], 8)).unwrap();
    let world = "minecraft:overworld";
    let outside = [4, 64, 0];
    let inside = [3, 64, 0];
    let owner = Actor {
        uuid: OWNER,
        name: "Owner".into(),
        operator: false,
    };
    let guest = Actor {
        uuid: GUEST,
        name: "Guest".into(),
        operator: false,
    };
    assert!(service.blocks_growth(world, outside, inside, true, false, None));
    assert!(!service.blocks_growth(world, inside, [2, 64, 0], true, false, None));
    assert!(service.blocks_growth(world, inside, [0, 64, 0], true, false, None));
    assert!(service.blocks_growth(world, outside, inside, true, true, Some(&owner)));
    assert!(!service.blocks_growth(world, outside, inside, false, true, Some(&owner)));
    assert!(service.blocks_growth(world, inside, [2, 64, 0], true, true, Some(&guest)));
    assert!(service.blocks_growth(world, inside, [2, 64, 0], false, true, None));
    assert!(!service.blocks_growth(world, inside, outside, true, true, Some(&guest)));
    assert_eq!(service.denials.get(&Flag::Build), Some(&1));
}

#[test]
fn paper_vehicle_and_inventory_holders_use_container_permission() {
    for kind in [
        "minecraft:oak_boat",
        "minecraft:bamboo_chest_raft",
        "minecraft:spawner_minecart",
        "minecraft:pig",
        "minecraft:strider",
        "minecraft:donkey",
        "minecraft:camel_husk",
        "minecraft:trader_llama",
        "minecraft:nautilus",
        "minecraft:zombie_nautilus",
        "minecraft:happy_ghast",
    ] {
        assert!(crate::paper_vehicle(kind), "{kind}");
        assert_eq!(
            crate::entity_interaction(kind, Some("minecraft:lead".into())),
            Flag::Container,
            "{kind}"
        );
    }
    for kind in [
        "minecraft:allay",
        "minecraft:piglin",
        "minecraft:pillager",
        "minecraft:player",
    ] {
        assert!(!crate::paper_vehicle(kind), "{kind}");
        assert_eq!(crate::entity_interaction(kind, None), Flag::Container);
    }
    assert_eq!(
        crate::entity_interaction("minecraft:villager", None),
        Flag::Trade
    );
    assert_eq!(
        crate::entity_interaction("minecraft:cow", Some("minecraft:lead".into())),
        Flag::Leash
    );
    assert_eq!(
        crate::entity_interaction("minecraft:cow", None),
        Flag::Build
    );
}

#[test]
fn a_vehicle_boundary_checks_each_players_access_to_the_destination() {
    let dir = tempfile::tempdir().unwrap();
    let mut service = Service::default();
    service.start(dir.path().into());
    let mut first = area(1, [0, 64, 0], 8);
    first.flags.insert(Flag::Move, false);
    service.index.add(first).unwrap();
    service.index.add(area(2, [8, 64, 0], 8)).unwrap();
    let dim = "minecraft:overworld";
    let guest = || {
        Some(Actor {
            uuid: GUEST,
            name: "Guest".into(),
            operator: false,
        })
    };
    let owner = || {
        Some(Actor {
            uuid: OWNER,
            name: "Owner".into(),
            operator: false,
        })
    };
    assert!(!service.blocks_ridden_vehicle(dim, [-5, 64, 0], [-4, 64, 0], owner));
    assert!(service.blocks_ridden_vehicle(dim, [-5, 64, 0], [-4, 64, 0], guest));
    assert!(!service.blocks_ridden_vehicle(dim, [2, 64, 0], [3, 64, 0], guest));
    assert!(!service.blocks_ridden_vehicle(dim, [3, 64, 0], [4, 64, 0], guest));
    assert!(service.blocks_ridden_vehicle(dim, [4, 64, 0], [3, 64, 0], guest));
    assert!(!service.blocks_ridden_vehicle(dim, [-4, 64, 0], [-5, 64, 0], guest));
    assert_eq!(service.denials.get(&Flag::Move), Some(&2));
}

#[test]
fn the_piston_checks_fragile_blocks_and_papers_retract_directions() {
    let dir = tempfile::tempdir().unwrap();
    let mut service = Service::default();
    service.start(dir.path().into());
    service.index.add(area(1, [0, 64, 0], 8)).unwrap();
    let dim = "minecraft:overworld";
    let east = [1, 0, 0];
    let west = [-1, 0, 0];
    let base = [1, 63, 0];
    assert!(!service.blocks_piston(dim, base, east, [[2, 63, 0]].into_iter()));
    assert!(service.blocks_piston(dim, base, east, [[3, 63, 0]].into_iter()));
    assert!(service.blocks_piston(dim, base, east, [[0, 64, 0]].into_iter()));
    assert_eq!(crate::piston_protection_direction(east, true, true), east);
    assert_eq!(crate::piston_protection_direction(west, false, true), east);
    assert_eq!(crate::piston_protection_direction(west, false, false), west);
    // The empty retract faces east, and the Paper listener checks west of the base.
    assert!(!service.blocks_piston(dim, [3, 63, 0], west, std::iter::empty()));
    assert!(service.blocks_piston(dim, [-4, 63, 0], west, std::iter::empty()));
}

#[test]
fn physical_redstone_uses_player_interaction_or_natural_build_permission() {
    let dir = tempfile::tempdir().unwrap();
    let mut service = Service::default();
    service.start(dir.path().into());
    let mut claim = area(1, [0, 64, 0], 8);
    claim.flags.insert(Flag::RedstonePerform, true);
    service.index.add(claim).unwrap();
    let guest = Actor {
        uuid: GUEST,
        name: "Guest".into(),
        operator: false,
    };
    let dim = "minecraft:overworld";
    let plate = [1, 64, 0];
    let redstone = Flag::Redstone;
    assert!(service.blocks_physical_trigger(dim, plate, Some(&guest), redstone));
    assert!(service.blocks_physical_trigger(dim, plate, None, redstone));
    assert!(!service.blocks_physical_trigger(dim, [4, 64, 0], Some(&guest), redstone));
    service
        .index
        .areas
        .get_mut(&OWNER)
        .unwrap()
        .flags
        .insert(Flag::Redstone, true);
    assert!(!service.blocks_physical_trigger(dim, plate, Some(&guest), redstone));
    assert!(service.blocks_physical_trigger(dim, plate, None, redstone));
    service
        .index
        .areas
        .get_mut(&OWNER)
        .unwrap()
        .flags
        .insert(Flag::Build, true);
    assert!(!service.blocks_physical_trigger(dim, plate, None, redstone));
}

#[test]
fn turtle_eggs_use_natural_build_only_for_non_players() {
    let dir = tempfile::tempdir().unwrap();
    let mut service = Service::default();
    service.start(dir.path().into());
    let core = [0, 64, 0];
    let mut claim = area(1, core, 8);
    claim.flags.insert(Flag::Build, false);
    claim.flags.insert(Flag::Redstone, true);
    service.index.add(claim).unwrap();
    let dim = "minecraft:overworld";

    assert!(service.blocks_egg_trample(dim, core, false));
    assert!(service.blocks_egg_trample(dim, [1, 64, 0], false));
    assert!(!service.blocks_egg_trample(dim, core, true));
    assert!(!service.blocks_egg_trample(dim, [20, 64, 0], false));

    let claim = service.index.areas.get_mut(&OWNER).unwrap();
    claim.flags.insert(Flag::Build, true);
    claim.flags.insert(Flag::Redstone, false);
    assert!(!service.blocks_egg_trample(dim, core, false));
}

#[test]
fn standing_on_dripleaf_requires_build_and_preserves_a_core_even_for_its_owner() {
    let dir = tempfile::tempdir().unwrap();
    let mut service = Service::default();
    service.start(dir.path().into());
    let mut claim = area(1, [0, 64, 0], 8);
    claim.flags.insert(Flag::Redstone, true);
    service.index.add(claim).unwrap();
    let owner = Actor {
        uuid: OWNER,
        name: "Owner".into(),
        operator: false,
    };
    let guest = Actor {
        uuid: GUEST,
        name: "Guest".into(),
        operator: false,
    };
    let dim = "minecraft:overworld";
    let leaf = [1, 64, 0];
    let build = Flag::Build;
    assert!(!service.blocks_physical_trigger(dim, leaf, Some(&owner), build));
    assert!(service.blocks_physical_trigger(dim, leaf, Some(&guest), build));
    assert!(service.blocks_physical_trigger(dim, leaf, None, build));
    assert!(service.blocks_physical_trigger(dim, [0, 64, 0], Some(&owner), build));
    assert!(!service.blocks_physical_trigger(dim, [4, 64, 0], None, build));
}

#[test]
fn growth_spread_uses_natural_build_at_each_target_and_keeps_the_core_protected() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = crate::Mod::default();
    let mut service = plugin.service();
    service.start(dir.path().into());
    let mut claim = area(1, [0, 64, 0], 8);
    claim.flags.insert(Flag::Fire, true);
    service.index.add(claim).unwrap();
    drop(service);

    let dimension = "minecraft:overworld";
    let growth = |target| plugin.protect_natural(dimension, target, Flag::Build);
    assert_eq!(growth([1, 64, 0]), mod_api::Flow::Cancel);
    assert_eq!(growth([4, 64, 0]), mod_api::Flow::Continue);
    plugin
        .service()
        .index
        .areas
        .get_mut(&OWNER)
        .unwrap()
        .flags
        .insert(Flag::Build, true);
    assert_eq!(growth([1, 64, 0]), mod_api::Flow::Continue);
    assert_eq!(growth([0, 64, 0]), mod_api::Flow::Cancel);
}

#[test]
fn paper_block_clicks_use_the_classification_before_inventory_opening() {
    use crate::interaction_flag;
    assert_eq!(
        interaction_flag("minecraft:decorated_pot"),
        Some(Flag::Container)
    );
    assert_eq!(
        interaction_flag("minecraft:oak_shelf"),
        Some(Flag::Container)
    );
    assert_eq!(interaction_flag("minecraft:ender_chest"), None);
    assert_eq!(interaction_flag("minecraft:dragon_egg"), Some(Flag::Build));
    assert_eq!(interaction_flag("minecraft:iron_door"), Some(Flag::Door));
    assert_eq!(
        interaction_flag("minecraft:stone_pressure_plate"),
        Some(Flag::Redstone)
    );
}

#[test]
fn workbench_protects_the_dispenser_selected_by_the_actual_horizontal_structure() {
    use crate::slimefun_workbench_dispenser;
    const WORKBENCH: [i32; 3] = [3, 64, 0];
    const BELOW: [i32; 3] = [3, 63, 0];
    let east = [4, 64, 0];
    let south = [3, 64, 1];
    let west = [2, 64, 0];
    let north = [3, 64, -1];
    let mut blocks = BTreeMap::from([
        (east, "minecraft:dispenser"),
        (south, "minecraft:dispenser"),
        (west, "minecraft:bookshelf"),
        (north, "minecraft:bookshelf"),
        (BELOW, "minecraft:dispenser"),
    ]);
    let selected = |blocks: &BTreeMap<_, &str>| {
        slimefun_workbench_dispenser(WORKBENCH, |pos| {
            blocks.get(&pos).copied().map(str::to_owned)
        })
    };

    let dir = tempfile::tempdir().unwrap();
    let mut service = Service::default();
    service.start(dir.path().into());
    service.index.add(area(1, [8, 64, 0], 8)).unwrap();
    let guest = Actor {
        uuid: GUEST,
        name: "Guest".into(),
        operator: false,
    };
    let dim = "minecraft:overworld";
    assert_eq!(selected(&blocks), Some(east));
    assert!(!service.allowed(&guest, dim, selected(&blocks).unwrap(), Flag::Container));
    assert!(service.allowed(&guest, dim, BELOW, Flag::Container));

    blocks.remove(&west);
    assert_eq!(selected(&blocks), Some(east));
    blocks.remove(&east);
    assert_eq!(selected(&blocks), Some(south));
    blocks.insert(north, "minecraft:chiseled_bookshelf");
    assert_eq!(selected(&blocks), Some(BELOW));
    blocks.insert(east, "minecraft:bookshelf");
    blocks.insert(west, "minecraft:dispenser");
    blocks.remove(&south);
    assert_eq!(selected(&blocks), Some(west));
    blocks.insert(south, "minecraft:bookshelf");
    blocks.insert(north, "minecraft:dispenser");
    blocks.remove(&west);
    assert_eq!(selected(&blocks), Some(north));
    blocks.remove(&north);
    blocks.remove(&BELOW);
    assert_eq!(selected(&blocks), None);
}

#[test]
fn pressure_chamber_guard_requires_every_block_in_one_axis() {
    let center = [4, 65, -3];
    for axis in [0, 2] {
        let mut blocks = BTreeMap::from([
            ([4, 66, -3], "minecraft:glass"),
            ([4, 67, -3], "minecraft:dispenser"),
        ]);
        for dy in 0..3 {
            for side in [-1, 1] {
                let mut pos = [4, 65 + dy, -3];
                pos[axis] += side;
                blocks.insert(
                    pos,
                    if dy == 2 {
                        "minecraft:smooth_stone_slab"
                    } else {
                        "minecraft:piston"
                    },
                );
            }
        }
        assert_eq!(
            crate::pressure_chamber(center, |pos| blocks.get(&pos).map(|s| (*s).to_owned())),
            Some([4, 67, -3])
        );
        let mut broken = [4, 66, -3];
        broken[axis] -= 1;
        blocks.remove(&broken);
        assert_eq!(
            crate::pressure_chamber(center, |pos| blocks.get(&pos).map(|s| (*s).to_owned())),
            None
        );
    }
}

#[test]
fn pvp_uses_the_shooters_current_level_and_the_victims_block() {
    let dir = tempfile::tempdir().unwrap();
    let mut service = Service::default();
    service.start(dir.path().into());
    let mut claim = area(1, [0, 64, 0], 8);
    claim.flags.insert(Flag::Pvp, false);
    service.index.add(claim).unwrap();
    let overworld = "minecraft:overworld";
    let nether = "minecraft:the_nether";
    let inside = [0, 64, 0];
    let outside = [5, 64, 0];
    assert!(crate::combat::denies_pvp(
        &service,
        Some((overworld, inside)),
        nether,
        [5.0, 64.0, 0.0]
    ));
    assert!(crate::combat::denies_pvp(
        &service,
        Some((nether, outside)),
        overworld,
        [-3.1, 64.0, 0.0]
    ));
    assert!(!crate::combat::denies_pvp(
        &service,
        Some((nether, inside)),
        overworld,
        [5.0, 64.0, 0.0]
    ));
}

#[test]
fn location_targeted_pvp_uses_only_the_landing_blocks_natural_policy() {
    use crate::combat::{denies_pvp, denies_pvp_at};

    let dir = tempfile::tempdir().unwrap();
    let mut service = Service::default();
    let overworld = "minecraft:overworld";
    let nether = "minecraft:the_nether";
    let protected = [0, 64, 0];
    let open = [5, 64, 0];
    assert!(!denies_pvp_at(&service, overworld, protected));
    service.start(dir.path().into());
    let mut claim = area(1, protected, 8);
    claim.flags.insert(Flag::Pvp, false);
    service.index.add(claim).unwrap();

    assert!(denies_pvp_at(&service, overworld, protected));
    assert!(!denies_pvp_at(&service, nether, protected));
    assert!(!denies_pvp_at(&service, overworld, open));
    assert!(denies_pvp(
        &service,
        Some((overworld, protected)),
        overworld,
        open.map(f64::from),
    ));
}

#[test]
fn creature_spawn_flags_follow_living_entities_instead_of_spawn_categories() {
    use crate::creature_spawn_flag;

    for kind in [
        "minecraft:armor_stand",
        "minecraft:iron_golem",
        "minecraft:ender_dragon",
        "minecraft:slime",
    ] {
        assert_eq!(creature_spawn_flag(kind, true), Some(Flag::SpawnAnimals));
    }
    assert_eq!(
        creature_spawn_flag("minecraft:zombie", true),
        Some(Flag::SpawnMonster)
    );
    assert_eq!(creature_spawn_flag("minecraft:player", true), None);
    for kind in ["minecraft:item", "minecraft:oak_boat", "minecraft:arrow"] {
        assert_eq!(creature_spawn_flag(kind, false), None);
    }
}

#[test]
fn damage_uses_player_responsibility_or_natural_rules_by_victim_kind() {
    use crate::combat::denies_entity_damage as denied;
    assert_eq!(
        crate::combat::target_flag("minecraft:shulker", true),
        Flag::DamageAnimals
    );
    assert_eq!(
        crate::combat::target_flag("minecraft:ghast", true),
        Flag::DamageAnimals
    );
    assert_eq!(
        crate::combat::target_flag("minecraft:piglin", true),
        Flag::DamageMonster
    );
    use mod_api::entity::health::DamagePlayer;
    let dir = tempfile::tempdir().unwrap();
    let mut service = Service::default();
    service.start(dir.path().into());
    let mut claim = area(1, [0, 64, 0], 8);
    claim.flags.insert(Flag::Pvp, false);
    service.index.add(claim).unwrap();
    let dim = "minecraft:overworld";
    let inside = [0.0, 64.0, 0.0];
    let guest = DamagePlayer {
        uuid: GUEST.as_u128(),
        name: "Guest".into(),
        permission: 0,
        dimension: dim.into(),
        position: [9.0, 64.0, 0.0],
    };
    let owner = DamagePlayer {
        uuid: OWNER.as_u128(),
        name: "Renamed".into(),
        ..guest.clone()
    };
    assert!(!denied(
        &mut service,
        dim,
        inside,
        "minecraft:player",
        true,
        None
    ));
    assert!(denied(
        &mut service,
        dim,
        inside,
        "minecraft:player",
        true,
        Some(&owner)
    ));
    assert!(!denied(
        &mut service,
        dim,
        inside,
        "minecraft:pig",
        true,
        None
    ));
    assert!(denied(
        &mut service,
        dim,
        inside,
        "minecraft:pig",
        true,
        Some(&guest)
    ));
    assert!(!denied(
        &mut service,
        dim,
        inside,
        "minecraft:pig",
        true,
        Some(&owner)
    ));
    assert!(denied(
        &mut service,
        dim,
        inside,
        "minecraft:oak_boat",
        false,
        None
    ));
    assert!(denied(
        &mut service,
        dim,
        inside,
        "minecraft:oak_boat",
        false,
        Some(&guest)
    ));
    assert!(!denied(
        &mut service,
        dim,
        inside,
        "minecraft:oak_boat",
        false,
        Some(&owner)
    ));
}

#[test]
fn owner_uuid_survives_rename_and_member_uses_independent_defaults() {
    let mut a = area(1, [0, 64, 0], 8);
    a.flags.insert(Flag::Build, true);
    a.members.insert(
        GUEST.to_string(),
        Member {
            name: "Member".into(),
            flags: BTreeMap::new(),
        },
    );
    assert!(a.allows(Flag::Build, OWNER, "OwnerRenamed", false));
    assert!(!a.is_owner(GUEST, "Owner"));
    assert!(!a.allows(Flag::Build, GUEST, "MemberRenamed", false));
    assert!(a.allows(Flag::Build, Uuid::from_u128(3), "Guest", false));
    a.members
        .get_mut(&GUEST.to_string())
        .unwrap()
        .flags
        .insert(Flag::Build, true);
    assert!(a.allows(Flag::Build, GUEST, "Member", false));
}

#[test]
fn schema_two_keeps_inclusive_legacy_radius_and_upgrades_to_schema_three() {
    let dir = tempfile::tempdir().unwrap();
    let original = "schema: 2
areas:
  00000000-0000-0000-0000-000000000001:
    world: world
    owner-uuid: 00000000-0000-0000-0000-000000000002
    owner-name: Owner
    volumes:
    - {x: -16, y: 64, z: 0, rx: 8, ry: 8, rz: 8, core: '0'}
    flags: {build: false}
    members:
      00000000-0000-0000-0000-000000000003:
        name: Member
        flags: {build: true}
";
    fs::write(dir.path().join("areas.yml"), original).unwrap();
    let store = Store::new(dir.path().into());
    let one = store.load().unwrap();
    let record = one.areas.get(&OWNER).unwrap();
    assert_eq!(record.volumes[0].min, [-24, 56, -8]);
    assert_eq!(record.volumes[0].max, [-8, 72, 8]);
    store.save(&one).unwrap();
    assert_eq!(store.load().unwrap().areas, one.areas);
    assert!(
        fs::read_to_string(dir.path().join("areas.yml"))
            .unwrap()
            .starts_with("schema: 3")
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("areas.yml.bak")).unwrap(),
        original
    );
}

#[test]
fn corrupt_record_prevents_load_and_preserves_original() {
    let dir = tempfile::tempdir().unwrap();
    let bad = "schema: 3\nareas: [bad]";
    fs::write(dir.path().join("areas.yml"), bad).unwrap();
    let mut service = Service::default();
    service.start(dir.path().into());
    assert!(service.error.is_some());
    assert!(service.save().is_err());
    let guest = Actor {
        uuid: GUEST,
        name: "Guest".into(),
        operator: false,
    };
    assert!(!service.allowed(&guest, "minecraft:overworld", [0, 64, 0], Flag::Build));
    assert_eq!(
        fs::read_to_string(dir.path().join("areas.yml")).unwrap(),
        bad
    );
    assert!(parse("schema: 42\nareas: {}").is_err());
}

#[test]
fn failed_mutation_keeps_in_memory_protected_land() {
    let dir = tempfile::tempdir().unwrap();
    let mut service = Service::default();
    service.start(dir.path().into());
    service
        .change(|index, _| index.add(area(1, [0, 64, 0], 8)))
        .unwrap();
    fs::create_dir(dir.path().join("areas.yml.bak")).unwrap();
    assert!(service.remove_core(OWNER, [0, 64, 0]).is_err());
    assert!(service.at("minecraft:overworld", [0, 64, 0]).is_some());
}

#[test]
fn one_protected_candidate_blocks_the_batch_and_preserves_core_when_explosions_are_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let mut service = Service::default();
    service.start(dir.path().into());
    service.index.add(area(1, [0, 64, 0], 8)).unwrap();
    assert!(!service.explosion("minecraft:overworld", [0.0, 65.0, 0.0], &[]));
    assert!(service.explosions[0].contains("候选方块 0 | 受保护 0"));
    assert!(service.explosion(
        "minecraft:overworld",
        [4.0, 65.0, 0.0],
        &[[4, 64, 0], [3, 64, 0]]
    ));
    assert!(!service.explosion(
        "minecraft:overworld",
        [4.0, 65.0, 0.0],
        &[[4, 64, 0], [5, 64, 0]]
    ));
    service
        .index
        .areas
        .get_mut(&OWNER)
        .unwrap()
        .flags
        .insert(Flag::Explosion, true);
    assert!(!service.explosion(
        "minecraft:overworld",
        [0.0, 65.0, 0.0],
        &[[1, 64, 0], [2, 64, 0]]
    ));
    assert!(service.explosion(
        "minecraft:overworld",
        [0.0, 65.0, 0.0],
        &[[1, 64, 0], [0, 64, 0]]
    ));
    assert!(service.explosions[0].contains("首个命中 0,64,0"));
    for _ in 0..20 {
        assert!(!service.explosion("minecraft:the_nether", [0.0, 65.0, 0.0], &[[0, 64, 0]]));
    }
    assert_eq!(service.explosions.len(), 10);
}

#[test]
fn wind_burst_checks_claims_before_later_block_listeners() {
    use crate::model::Point;
    use mod_api::{
        Backend, BackendRef, BlockKey, BlockState, BlockStateRecord, Hooks, Host, LevelRef,
        ModEntry, ServerHost, WindBurstEvent,
        prelude::{EntitiesHost, KeysHost, LevelsBackend},
        testing::{FakeHost, with_dispatch},
    };
    use std::sync::{Arc, Mutex};

    const WORLD: &str = "minecraft:overworld";
    const CORE: Point = [0, 64, 0];
    const AIR: Point = [2, 64, 0];
    const INSIDE: Point = [3, 64, 0];
    const OUTSIDE: Point = [4, 64, 0];

    // FakeHost's block table is unresolved in this unit test. Supply the
    // candidate cells and distinguish explicit air from an unloaded cell.
    struct BurstHost(LevelRef);
    impl Host for BurstHost {
        fn backend(&self) -> Option<BackendRef<'_>> {
            Some(mod_api::__private::parts_installation::backend_ref(self))
        }
    }
    impl EntitiesHost for BurstHost {
        fn level(&self, dimension: &str) -> Option<LevelRef> {
            (dimension == WORLD).then_some(self.0)
        }
    }
    impl KeysHost for BurstHost {
        fn block_key(&self, id: &str) -> Option<BlockKey> {
            let slot = match id {
                "minecraft:air" => 0,
                "minecraft:cave_air" => 1,
                "minecraft:void_air" => 2,
                "minecraft:stone" => 3,
                _ => return None,
            };
            Some(mod_api::__private::parts_installation::block_key(slot))
        }
    }
    mod_api::impl_host_areas!(BurstHost, except(entities, keys));
    impl Backend for BurstHost {}
    impl LevelsBackend for BurstHost {
        fn level_block_state(&self, level: LevelRef, pos: Point) -> Option<BlockState> {
            if level != self.0 {
                return None;
            }
            let id = match pos {
                AIR => "minecraft:cave_air",
                CORE | INSIDE | OUTSIDE => "minecraft:stone",
                _ => return None,
            };
            Some(BlockStateRecord::new(self.block_key(id)?, 0).into())
        }
    }
    mod_api::impl_backend_areas!(BurstHost, except(levels));
    impl ServerHost for BurstHost {}
    mod_api::impl_server_host_areas!(BurstHost);

    struct LaterListener(Arc<Mutex<Vec<Vec<Point>>>>);

    #[mod_api::__subscriptions]
    impl Hooks for LaterListener {
        fn wind_burst(&self, _: &mut dyn ServerHost, event: &mut WindBurstEvent<'_>) {
            self.0.lock().unwrap().push(event.blocks.clone());
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let plugin = crate::Mod::default();
    let claims = plugin.service.clone();
    {
        let mut service = plugin.service();
        service.start(dir.path().into());
        let mut claim = area(1, CORE, 8);
        claim.flags.insert(Flag::Explosion, false);
        service.index.add(claim).unwrap();
    }
    mod_api::__private::initialize().unwrap();
    let _phase = mod_api::__private::PhaseOwner::scope(mod_api::__private::ModPhase::World);
    let mut host = BurstHost(FakeHost::new().level.level());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mods = [
        ModEntry::new("observer", Box::new(LaterListener(seen.clone()))),
        ModEntry::new("purtmarsarea", Box::new(plugin)),
    ]
    .into();
    with_dispatch(mods, || {
        let mut wind = |positions: [Point; 2], expected: &[Point]| {
            let mut blocks = positions.to_vec();
            mod_api::dispatch_wind_burst(
                &mut host,
                WORLD,
                [4.0, 64.5, 0.5],
                &mut blocks,
            );
            assert_eq!(blocks, expected);
            assert_eq!(seen.lock().unwrap().last().unwrap(), expected);
        };
        wind([OUTSIDE, AIR], &[OUTSIDE, AIR]);
        wind([OUTSIDE, INSIDE], &[]);
        claims
            .lock()
            .unwrap()
            .index
            .areas
            .get_mut(&OWNER)
            .unwrap()
            .flags
            .insert(Flag::Explosion, true);
        wind([OUTSIDE, INSIDE], &[OUTSIDE, INSIDE]);
        wind([OUTSIDE, CORE], &[]);
    });
}

#[test]
fn core_creation_enforces_world_height_limits_and_overlap() {
    let dir = tempfile::tempdir().unwrap();
    let mut service = Service::default();
    service.start(dir.path().into());
    let owner = Actor {
        uuid: OWNER,
        name: "Owner".into(),
        operator: false,
    };
    assert!(
        service
            .validate_create(&owner, "minecraft:the_nether", [0, 64, 0], "0", 0..256)
            .is_err()
    );
    assert!(
        service
            .validate_create(&owner, "minecraft:overworld", [0, 1, 0], "0", 0..256)
            .is_err()
    );
    let a = service
        .validate_create(&owner, "minecraft:overworld", [0, 64, 0], "0", 0..256)
        .unwrap();
    assert_eq!(a.volumes[0].max, [3, 67, 3]);
    service.index.add(a).unwrap();
    assert!(
        service
            .validate_create(&owner, "minecraft:overworld", [7, 64, 0], "0", 0..256)
            .is_err()
    );
    assert!(
        service
            .validate_create(&owner, "minecraft:overworld", [8, 64, 0], "0", 0..256)
            .is_ok()
    );
}

#[test]
fn starter_crafts_with_a_chest_ring_and_existing_config_adds_the_exact_core() {
    let recipe: serde_yaml_ng::Value = serde_yaml_ng::from_slice(crate::STARTER_RECIPE).unwrap();
    let rows: Vec<_> = recipe["pattern"].as_sequence().unwrap().iter()
        .map(|row| row.as_str().unwrap()).collect();
    assert_eq!(rows, ["CCC", "CGC", "CCC"]);
    assert_eq!(recipe["key"]["C"].as_str(), Some("minecraft:chest"));
    assert_eq!(recipe["key"]["G"].as_str(), Some("minecraft:glass"));
    assert_eq!(recipe["result"]["components"]["minecraft:custom_data"]
        ["PublicBukkitValues"]["purtmarsarea:core"].as_str(), Some(crate::store::STARTER_CORE));

    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("config.yml"), "Settings: {}\nCores:\n  '0':\n    dimensions: {x: 8, y: 8, z: 8}\n").unwrap();
    let config = Store::new(dir.path().into()).config().unwrap();
    let core = &config.cores[crate::store::STARTER_CORE];
    assert_eq!(Some(crate::cores::item_id(&core.item).as_str()), recipe["result"]["id"].as_str());
    assert_eq!(core.volume(crate::store::STARTER_CORE, [0, 64, 0], &config.settings).unwrap().sides(), [4; 3]);
    assert_eq!(config.cores["0"].volume("0", [0, 64, 0], &config.settings).unwrap().sides(), [8; 3]);
    assert_eq!(config.settings.max_expansion_side_length, 64);
}

#[test]
fn directional_expansion_respects_shared_horizontal_limit_and_all_dimension_height() {
    use ExpansionDirection::{Down, East, North, Up, West};
    let start = Volume::sized([0, 64, 0], [4; 3], "starter".into()).unwrap();
    let overworld = -64..320;
    let east = start.expanded(East, 64, overworld.clone()).unwrap();
    assert_eq!(east.sides(), [8, 4, 4]);
    assert_eq!((east.min, east.max), (start.min, [5, 65, 1]));
    let capped = east.expanded(West, 9, overworld.clone()).unwrap();
    assert_eq!(capped.sides(), [9, 4, 4]);
    assert!(capped.expanded(East, 9, overworld.clone()).is_err());
    assert!(capped.expanded(North, 9, overworld.clone()).is_ok());

    let mut tall = capped;
    for direction in [Up, Down] {
        while let Ok(next) = tall.expanded(direction, 9, overworld.clone()) {
            tall = next;
        }
    }
    assert_eq!((tall.min[1], tall.max[1], tall.sides()), (-64, 319, [9, 384, 4]));
    assert_eq!(tall.core_pos, start.core_pos);
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().into());
    let mut index = AreaIndex::default();
    let mut saved = area(1, [0, 64, 0], 4);
    saved.volumes = vec![tall.clone()];
    index.add(saved).unwrap();
    store.save(&index).unwrap();
    assert_eq!(store.load().unwrap().areas[&OWNER].volumes[0], tall);
}

#[test]
fn legacy_import_is_deterministic_and_rejects_partial_import() {
    let dir = tempfile::tempdir().unwrap();
    let legacy = dir.path().join("legacy-save");
    fs::create_dir(&legacy).unwrap();
    fs::write(legacy.join("world.yml"), "0,64,0: {owner: Owner, size: {x: 8, y: 8, z: 8}, index: 0}\n5,64,0: {owner: Guest, size: {x: 8, y: 8, z: 8}, index: 0}\n").unwrap();
    let mut service = Service::default();
    service.start(dir.path().into());
    assert!(service.import_legacy(&[]).is_err());
    assert!(service.index.areas.is_empty());
    fs::write(
        legacy.join("world.yml"),
        "0,64,0: {owner: Owner, size: {x: 8, y: 8, z: 8}, index: 0}\n",
    )
    .unwrap();
    assert_eq!(service.import_legacy(&[]).unwrap(), 1);
    assert_eq!(service.import_legacy(&[]).unwrap(), 0);
    assert_eq!(service.index.areas.len(), 1);
}

#[test]
fn permission_nodes_separate_admin_bypass_and_world_placement() {
    let dir = tempfile::tempdir().unwrap();
    let mut service = Service::default();
    service.start(dir.path().into());
    service.index.add(area(1, [0, 64, 0], 8)).unwrap();
    let guest = Actor {
        uuid: GUEST,
        name: "Guest".into(),
        operator: false,
    };
    let settings = &mut service.config.as_mut().unwrap().settings;
    settings.permission_nodes.insert(
        GUEST,
        BTreeMap::from([
            ("PurtmarsArea.command.admin".into(), true),
            ("PurtmarsArea.bypass.world_nether".into(), true),
        ]),
    );
    assert!(guest.manages(settings, service.index.areas.get(&OWNER).unwrap()));
    assert!(!service.allowed(&guest, "minecraft:overworld", [0, 64, 0], Flag::Build));
    assert!(
        service
            .validate_create(&guest, "minecraft:the_nether", [0, 64, 0], "0", 0..256)
            .is_ok()
    );
    service
        .config
        .as_mut()
        .unwrap()
        .settings
        .permission_nodes
        .get_mut(&GUEST)
        .unwrap()
        .insert("PurtmarsArea.create".into(), false);
    assert!(
        service
            .validate_create(&guest, "minecraft:the_nether", [0, 64, 0], "0", 0..256)
            .is_err()
    );
}

#[test]
fn first_join_binds_legacy_owner_and_member_and_keeps_uuid_after_rename() {
    let dir = tempfile::tempdir().unwrap();
    let legacy = dir.path().join("legacy-save");
    fs::create_dir(&legacy).unwrap();
    fs::write(
        legacy.join("world.yml"),
        "0,64,0: {owner: Owner, size: {x: 8, y: 8, z: 8}, members: {Member: {build: true}}}\n",
    )
    .unwrap();
    let mut service = Service::default();
    service.start(dir.path().into());
    service.import_legacy(&[(OWNER, "Owner".into())]).unwrap();
    service.remember(GUEST, "mEmBeR").unwrap();
    service.remember(GUEST, "RenamedMember").unwrap();
    service.remember(OWNER, "RenamedOwner").unwrap();

    let mut reopened = Service::default();
    reopened.start(dir.path().into());
    let area = reopened.index.at("world", [0, 64, 0]).unwrap();
    assert_eq!(area.owner, Some(OWNER));
    assert!(area.is_owner(OWNER, "RenamedOwner"));
    assert!(!area.is_owner(GUEST, "Owner"));
    assert!(area.allows(Flag::Build, GUEST, "RenamedMember", false));
    assert!(!area.members.contains_key("member"));
    assert_eq!(reopened.known_players["renamedmember"].0, GUEST);
    assert!(!reopened.known_players.contains_key("member"));
}

#[test]
fn usercache_migration_requires_a_matching_playerdata_record() {
    let dir = tempfile::tempdir().unwrap();
    let world = dir.path().join("world");
    let data = world.join("players/data");
    fs::create_dir_all(&data).unwrap();
    fs::write(data.join(format!("{OWNER}.dat")), []).unwrap();
    fs::write(
        dir.path().join("usercache.json"),
        format!(
            r#"[{{"name":"Owner","uuid":"{OWNER}"}},{{"name":"NeverJoined","uuid":"{GUEST}"}}]"#
        ),
    )
    .unwrap();
    let mut service = Service::default();
    service.start(dir.path().join("plugin"));
    service.seed_known(dir.path(), &world).unwrap();
    assert_eq!(service.known_players["owner"].0, OWNER);
    assert!(!service.known_players.contains_key("neverjoined"));
}

#[test]
fn boundary_particles_keep_paper_edges_range_and_budget() {
    let v = Volume::sized([0, 64, 0], [8; 3], "0".into()).unwrap();
    let settings = crate::store::Particle {
        period: 2,
        distance: 2.0,
        range: 50.0,
        budget: 2,
    };
    assert_eq!(
        crate::presentation::boundary_points([&v].into_iter(), [0.0, 64.0, 0.0], &settings),
        vec![[-4.0, 60.0, -4.0], [-2.0, 60.0, -4.0]]
    );
    assert!(
        crate::presentation::boundary_points([&v].into_iter(), [100.0, 64.0, 0.0], &settings)
            .is_empty()
    );
    let tall = Volume::new([0, 64, 0], [-2, -64, -2], [1, 319, 1], "starter".into()).unwrap();
    assert!(!crate::presentation::boundary_points([&tall].into_iter(), [0.0, 300.0, 0.0], &settings).is_empty());
}
