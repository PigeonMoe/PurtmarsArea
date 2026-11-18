use crate::model::{Area, AreaIndex, Flag, Member, Volume};
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
fn overlap_rules_cover_the_whole_cuboid() {
    let mut index = AreaIndex::default();
    index.add(area(1, [0, 64, 0], 64)).unwrap();
    assert!(index.add(area(2, [0, 64, 0], 8)).is_err());
    assert!(index.add(area(2, [35, 64, 0], 8)).is_err());
    index.add(area(2, [36, 64, 0], 8)).unwrap();
    index.add(area(3, [0, 100, 0], 8)).unwrap();
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
    assert!(service.blocks_physical_redstone(dim, plate, Some(&guest)));
    assert!(service.blocks_physical_redstone(dim, plate, None));
    assert!(!service.blocks_physical_redstone(dim, [4, 64, 0], Some(&guest)));
    service
        .index
        .areas
        .get_mut(&OWNER)
        .unwrap()
        .flags
        .insert(Flag::Redstone, true);
    assert!(!service.blocks_physical_redstone(dim, plate, Some(&guest)));
    assert!(service.blocks_physical_redstone(dim, plate, None));
    service
        .index
        .areas
        .get_mut(&OWNER)
        .unwrap()
        .flags
        .insert(Flag::Build, true);
    assert!(!service.blocks_physical_redstone(dim, plate, None));
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
}
