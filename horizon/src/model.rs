use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use uuid::Uuid;

pub type Point = [i32; 3];
pub type Rules = BTreeMap<Flag, bool>;

macro_rules! flags {
    ($($name:ident, $key:literal, $title:literal, $default:literal, $personal:literal);+ $(;)?) => {
        #[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
        pub enum Flag {
            $(#[serde(rename = $key)] $name,)+
        }

        impl Flag {
            pub const ALL: &[Self] = &[$(Self::$name),+];

            pub fn key(self) -> &'static str {
                match self { $(Self::$name => $key,)+ }
            }

            pub fn title(self) -> &'static str {
                match self { $(Self::$name => $title,)+ }
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
    Particle, "particle", "边界粒子", true, false;
    Build, "build", "建筑与水桶", false, true;
    Move, "move", "进入与移动", true, true;
    Teleport, "teleport", "传送", true, true;
    Container, "container", "容器", false, true;
    Enchant, "enchant-table", "附魔台", true, true;
    Anvil, "anvil", "铁砧", true, true;
    Door, "door", "门与活板门", false, true;
    Cake, "cake", "蛋糕", false, true;
    Drop, "item-drop", "丢弃物品", true, true;
    Pick, "item-pick", "拾取物品与箭", true, true;
    DamageAnimals, "damage-animals", "伤害被动生物", false, true;
    DamageMonster, "damage-monster", "伤害敌对生物", false, true;
    SpawnAnimals, "spawn-animals", "被动生物生成", false, false;
    SpawnMonster, "spawn-monster", "敌对生物生成", false, false;
    Redstone, "redstone", "红石触发", false, true;
    RedstonePerform, "redstone-perform", "红石运作", true, false;
    Lava, "flow-lava", "岩浆流动", true, false;
    Water, "flow-water", "水流动", true, false;
    Trade, "villager-trade", "村民交易", true, true;
    Shear, "shear", "修剪生物", false, true;
    Leash, "leash", "牵引生物", false, true;
    Falling, "falling", "外来方块坠落", false, false;
    Explosion, "explosion", "爆炸破坏", false, false;
    Fire, "fire", "火焰蔓延与燃烧", false, false;
    Pvp, "pvp", "玩家战斗", true, false;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Volume {
    pub core_pos: Point,
    pub min: Point,
    pub max: Point,
    pub core: String,
}

#[derive(Clone, Copy, Debug)]
pub enum ExpansionDirection {
    West,
    East,
    Down,
    Up,
    North,
    South,
}

impl ExpansionDirection {
    pub fn title(self) -> &'static str {
        match self {
            Self::West => "向西扩建（X−）",
            Self::East => "向东扩建（X+）",
            Self::Down => "向下扩建（Y−）",
            Self::Up => "向上扩建（Y+）",
            Self::North => "向北扩建（Z−）",
            Self::South => "向南扩建（Z+）",
        }
    }

    fn face(self) -> (usize, bool) {
        match self {
            Self::West => (0, false),
            Self::East => (0, true),
            Self::Down => (1, false),
            Self::Up => (1, true),
            Self::North => (2, false),
            Self::South => (2, true),
        }
    }
}

impl Volume {
    pub fn new(core_pos: Point, min: Point, max: Point, core: String) -> Result<Self, String> {
        let [x, y, z] = core_pos.map(i32::unsigned_abs);
        if x > 30_000_000 || z > 30_000_000 || y > 2048 {
            return Err("坐标超出范围".into());
        }
        if (0..3).any(|axis| {
            // Old radius 128 saves have 257 inclusive horizontal blocks. A modern
            // dimension may be as tall as Minecraft's 4064-block dimension limit.
            let limit = if axis == 1 { 4064 } else { 257 };
            min[axis] > core_pos[axis]
                || max[axis] < core_pos[axis]
                || i64::from(max[axis]) - i64::from(min[axis]) >= limit
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

    pub fn sides(&self) -> Point {
        std::array::from_fn(|axis| self.max[axis] - self.min[axis] + 1)
    }

    pub fn expanded(
        &self,
        direction: ExpansionDirection,
        horizontal_limit: i32,
        height: std::ops::Range<i32>,
    ) -> Result<Self, String> {
        const EXPANSION_PER_KIT: i32 = 4;
        let (axis, positive) = direction.face();
        let mut min = self.min;
        let mut max = self.max;
        if !height.contains(&min[1]) || !height.contains(&max[1]) {
            return Err("现有领地超出了当前维度的建筑高度".into());
        }
        let amount = if axis == 1 {
            let remaining = if positive {
                height.end - 1 - max[1]
            } else {
                min[1] - height.start
            };
            if remaining == 0 {
                return Err("这个方向已到达维度的建筑高度限制".into());
            }
            EXPANSION_PER_KIT.min(remaining)
        } else {
            let remaining = horizontal_limit - self.sides()[axis];
            if remaining <= 0 {
                return Err(format!("这条水平边已到达 {horizontal_limit} 格上限"));
            }
            let room = if positive {
                29_999_999 - max[axis]
            } else {
                min[axis] + 30_000_000
            };
            if room <= 0 {
                return Err("这个方向已到达世界坐标边界".into());
            }
            EXPANSION_PER_KIT.min(remaining).min(room)
        };
        if positive {
            max[axis] += amount;
        } else {
            min[axis] -= amount;
        }
        Self::new(self.core_pos, min, max, self.core.clone())
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
        self.overlaps_except(world, volume, None)
    }

    pub fn overlaps_except(
        &self,
        world: &str,
        volume: &Volume,
        except: Option<(Uuid, Point)>,
    ) -> bool {
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
                    .any(|other| except != Some((*id, other.core_pos)) && volume.intersects(other))
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
