use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use uuid::Uuid;

pub type Point = [i32; 3];
pub type Rules = BTreeMap<Flag, bool>;

macro_rules! flags {
    ($($name:ident, $key:literal, $default:literal, $personal:literal);+ $(;)?) => {
        #[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
        pub enum Flag {
            $(#[serde(rename = $key)] $name,)+
        }

        impl Flag {
            pub const ALL: &[Self] = &[$(Self::$name),+];

            pub fn key(self) -> &'static str {
                match self { $(Self::$name => $key,)+ }
            }

            pub fn default_value(self) -> bool {
                match self { $(Self::$name => $default,)+ }
            }

            pub fn personal(self) -> bool {
                match self { $(Self::$name => $personal,)+ }
            }

            pub fn parse(key: &str) -> Result<Self, String> {
                Self::ALL
                    .iter()
                    .copied()
                    .find(|flag| flag.key().eq_ignore_ascii_case(key))
                    .ok_or_else(|| format!("未知权限：{key}"))
            }
        }
    };
}

flags! {
    Particle, "particle", true, false;
    Build, "build", false, true;
    Move, "move", true, true;
    Teleport, "teleport", true, true;
    Container, "container", false, true;
    Enchant, "enchant-table", true, true;
    Anvil, "anvil", true, true;
    Door, "door", false, true;
    Cake, "cake", false, true;
    Drop, "item-drop", true, true;
    Pick, "item-pick", true, true;
    DamageAnimals, "damage-animals", false, true;
    DamageMonster, "damage-monster", false, true;
    SpawnAnimals, "spawn-animals", false, false;
    SpawnMonster, "spawn-monster", false, false;
    Redstone, "redstone", false, true;
    RedstonePerform, "redstone-perform", true, false;
    Lava, "flow-lava", true, false;
    Water, "flow-water", true, false;
    Trade, "villager-trade", true, true;
    Shear, "shear", false, true;
    Leash, "leash", false, true;
    Falling, "falling", false, false;
    Explosion, "explosion", false, false;
    Fire, "fire", false, false;
    Pvp, "pvp", true, false;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Volume {
    pub core_pos: Point,
    pub min: Point,
    pub max: Point,
    pub core: String,
}

impl Volume {
    pub fn new(core_pos: Point, min: Point, max: Point, core: String) -> Result<Self, String> {
        let [x, y, z] = core_pos.map(i32::unsigned_abs);
        if x > 30_000_000 || z > 30_000_000 || y > 2048 {
            return Err("坐标超出范围".into());
        }
        if (0..3).any(|axis| {
            min[axis] > core_pos[axis]
                || max[axis] < core_pos[axis]
                || i64::from(max[axis]) - i64::from(min[axis]) >= 257
        }) {
            return Err("无效的领地边界".into());
        }
        Ok(Self {
            core_pos,
            min,
            max,
            core,
        })
    }

    pub fn sized(pos: Point, sides: Point, core: String) -> Result<Self, String> {
        if sides.iter().any(|side| !(1..=256).contains(side)) {
            return Err("领地边长必须为 1–256 个方块".into());
        }
        let min = std::array::from_fn(|i| pos[i].saturating_sub(sides[i] / 2));
        let max = std::array::from_fn(|i| pos[i].saturating_add((sides[i] - 1) / 2));
        Self::new(pos, min, max, core)
    }

    pub fn legacy(pos: Point, radii: Point, core: String) -> Result<Self, String> {
        if radii.iter().any(|radius| !(1..=128).contains(radius)) {
            return Err("旧格式领地半径必须为 1–128".into());
        }
        let min = std::array::from_fn(|i| pos[i].saturating_sub(radii[i]));
        let max = std::array::from_fn(|i| pos[i].saturating_add(radii[i]));
        Self::new(pos, min, max, core)
    }

    pub fn contains(&self, pos: Point) -> bool {
        (0..3).all(|i| (self.min[i]..=self.max[i]).contains(&pos[i]))
    }

    fn intersects(&self, other: &Self) -> bool {
        (0..3).all(|i| self.min[i] <= other.max[i] && other.min[i] <= self.max[i])
    }

    fn chunks(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        (self.min[0] >> 4..=self.max[0] >> 4)
            .flat_map(|x| (self.min[2] >> 4..=self.max[2] >> 4).map(move |z| (x, z)))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Member {
    pub name: String,
    #[serde(default, skip_serializing_if = "Rules::is_empty")]
    pub flags: Rules,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Area {
    pub id: Uuid,
    pub world: String,
    pub owner: Option<Uuid>,
    pub owner_name: String,
    pub name: String,
    pub welcome: String,
    pub farewell: String,
    pub volumes: Vec<Volume>,
    pub flags: Rules,
    pub members: BTreeMap<String, Member>,
}

impl Area {
    pub fn new(
        id: Uuid,
        world: String,
        owner: Option<Uuid>,
        owner_name: String,
        volumes: Vec<Volume>,
    ) -> Self {
        Self {
            id,
            world,
            owner,
            name: format!("{owner_name}的领地"),
            owner_name,
            welcome: "欢迎来到 {name}".into(),
            farewell: "离开 {name}".into(),
            volumes,
            flags: Rules::new(),
            members: BTreeMap::new(),
        }
    }

    pub fn is_owner(&self, uuid: Uuid, name: &str) -> bool {
        self.owner.map_or_else(
            || self.owner_name.eq_ignore_ascii_case(name),
            |owner| owner == uuid,
        )
    }

    pub fn member(&self, uuid: Uuid, name: &str) -> Option<&Member> {
        self.members
            .get(&uuid.to_string())
            .or_else(|| self.members.get(&name.to_ascii_lowercase()))
    }

    pub fn enabled(&self, flag: Flag, default: bool) -> bool {
        self.flags.get(&flag).copied().unwrap_or(default)
    }

    pub fn allows(&self, flag: Flag, uuid: Uuid, name: &str, default: bool) -> bool {
        if self.is_owner(uuid, name) {
            return true;
        }
        if flag.personal()
            && let Some(member) = self.member(uuid, name)
        {
            return member.flags.get(&flag).copied().unwrap_or(default);
        }
        self.enabled(flag, default)
    }

    pub fn core(&self, pos: Point) -> Option<&Volume> {
        self.volumes.iter().find(|volume| volume.core_pos == pos)
    }
}

#[derive(Clone, Default)]
pub struct AreaIndex {
    pub areas: BTreeMap<Uuid, Area>,
    chunks: HashMap<String, HashMap<(i32, i32), BTreeSet<Uuid>>>,
}

impl AreaIndex {
    pub fn at(&self, world: &str, pos: Point) -> Option<&Area> {
        self.chunks
            .get(world)?
            .get(&(pos[0] >> 4, pos[2] >> 4))?
            .iter()
            .filter_map(|id| self.areas.get(id))
            .find(|area| area.volumes.iter().any(|volume| volume.contains(pos)))
    }

    pub fn overlaps(&self, world: &str, volume: &Volume) -> bool {
        let Some(chunks) = self.chunks.get(world) else {
            return false;
        };
        volume
            .chunks()
            .filter_map(|chunk| chunks.get(&chunk))
            .flatten()
            .any(|id| {
                self.areas[id]
                    .volumes
                    .iter()
                    .any(|other| volume.intersects(other))
            })
    }

    pub fn add(&mut self, area: Area) -> Result<(), String> {
        if self.areas.contains_key(&area.id) {
            return Err(format!("重复领地 ID：{}", area.id));
        }
        if area
            .volumes
            .iter()
            .any(|volume| self.overlaps(&area.world, volume))
        {
            return Err(format!("领地与已有领地相交：{}", area.id));
        }
        let chunks = self.chunks.entry(area.world.clone()).or_default();
        for chunk in area.volumes.iter().flat_map(Volume::chunks) {
            chunks.entry(chunk).or_default().insert(area.id);
        }
        self.areas.insert(area.id, area);
        Ok(())
    }

    pub fn remove(&mut self, id: Uuid) -> Option<Area> {
        let area = self.areas.remove(&id)?;
        if let Some(chunks) = self.chunks.get_mut(&area.world) {
            for chunk in area.volumes.iter().flat_map(Volume::chunks) {
                if let Some(entries) = chunks.get_mut(&chunk) {
                    entries.remove(&id);
                    if entries.is_empty() {
                        chunks.remove(&chunk);
                    }
                }
            }
        }
        Some(area)
    }
}

pub fn player_name(name: &str) -> bool {
    (1..=16).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}
