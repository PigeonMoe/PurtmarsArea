use std::collections::BTreeMap;

use serde::Serialize;
use uuid::Uuid;

use crate::model::{Area, Member, Point, Rules, Volume};

pub(super) const LEAVE: &str = "purtmarsarea:area_leave";
pub(super) const ENTER: &str = "purtmarsarea:area_enter";

#[derive(Serialize)]
struct Payload<'a> {
    schema: u8,
    player: Player<'a>,
    area: AreaSnapshot<'a>,
}

#[derive(Serialize)]
struct Player<'a> {
    uuid: Uuid,
    name: &'a str,
}

#[derive(Serialize)]
struct AreaSnapshot<'a> {
    id: Uuid,
    world: &'a str,
    owner_uuid: Option<Uuid>,
    owner_name: &'a str,
    name: &'a str,
    welcome: &'a str,
    farewell: &'a str,
    volumes: Vec<VolumeSnapshot<'a>>,
    flags: &'a Rules,
    members: &'a BTreeMap<String, Member>,
}

#[derive(Serialize)]
struct VolumeSnapshot<'a> {
    core_pos: Point,
    min: Point,
    max: Point,
    core: &'a str,
}

impl<'a> From<&'a Volume> for VolumeSnapshot<'a> {
    fn from(volume: &'a Volume) -> Self {
        Self {
            core_pos: volume.core_pos,
            min: volume.min,
            max: volume.max,
            core: &volume.core,
        }
    }
}

pub(super) fn encode(player: Uuid, name: &str, area: &Area) -> Result<String, serde_json::Error> {
    serde_json::to_string(&Payload {
        schema: 1,
        player: Player { uuid: player, name },
        area: AreaSnapshot {
            id: area.id,
            world: &area.world,
            owner_uuid: area.owner,
            owner_name: &area.owner_name,
            name: &area.name,
            welcome: &area.welcome,
            farewell: &area.farewell,
            volumes: area.volumes.iter().map(VolumeSnapshot::from).collect(),
            flags: &area.flags,
            members: &area.members,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Flag;

    #[test]
    fn notifications_keep_claim_identity_permissions_and_inclusive_bounds() {
        let visitor = Uuid::from_u128(1);
        let owner = Uuid::from_u128(2);
        let id = Uuid::from_u128(3);
        let mut area = Area::new(
            id,
            "legacy_world".into(),
            Some(owner),
            "Owner".into(),
            vec![Volume::new([-5, 60, 0], [-8, 57, -3], [0, 62, 4], "0".into()).unwrap()],
        );
        area.name = "测试领地".into();
        area.flags.insert(Flag::Build, false);
        area.members.insert(
            visitor.to_string(),
            Member {
                name: "Visitor".into(),
                flags: [(Flag::Build, true)].into(),
            },
        );
        let payload: serde_json::Value =
            serde_json::from_str(&encode(visitor, "Visitor", &area).unwrap()).unwrap();
        assert_eq!(payload["schema"], 1);
        assert_eq!(payload["player"]["uuid"], visitor.to_string());
        assert_eq!(payload["area"]["id"], id.to_string());
        assert_eq!(payload["area"]["owner_uuid"], owner.to_string());
        assert_eq!(payload["area"]["name"], "测试领地");
        assert_eq!(payload["area"]["world"], "legacy_world");
        assert_eq!(
            payload["area"]["volumes"][0]["min"],
            serde_json::json!([-8, 57, -3])
        );
        assert_eq!(
            payload["area"]["volumes"][0]["max"],
            serde_json::json!([0, 62, 4])
        );
        assert_eq!(payload["area"]["flags"]["build"], false);
        assert_eq!(
            payload["area"]["members"][visitor.to_string()]["flags"]["build"],
            true
        );
    }
}
