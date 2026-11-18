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
        admin: false,
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
        admin: false,
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
    assert!(service.import_legacy().is_err());
    assert!(service.index.areas.is_empty());
    fs::write(
        legacy.join("world.yml"),
        "0,64,0: {owner: Owner, size: {x: 8, y: 8, z: 8}, index: 0}\n",
    )
    .unwrap();
    assert_eq!(service.import_legacy().unwrap(), 1);
    assert_eq!(service.import_legacy().unwrap(), 0);
    assert_eq!(service.index.areas.len(), 1);
}
