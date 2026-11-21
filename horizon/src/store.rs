use crate::model::{Area, AreaIndex, Flag, Member, Point, Rules, Volume, player_name};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use uuid::Uuid;

pub type KnownPlayers = BTreeMap<String, (Uuid, String)>;

const DEFAULT_CONFIG: &str = include_str!("../data/purtmarsarea/config.yml");
pub const STARTER_CORE: &str = "starter";

#[derive(Deserialize)]
pub struct Config {
    #[serde(rename = "Settings")]
    pub settings: Settings,
    #[serde(rename = "Cores")]
    pub cores: BTreeMap<String, Core>,
}

#[derive(Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Settings {
    pub enable_world: BTreeSet<String>,
    pub member_limit: usize,
    pub member_input_timeout: u64,
    pub particle: Particle,
    pub permission_nodes: BTreeMap<Uuid, BTreeMap<String, bool>>,
    pub area_limit: usize,
    pub max_radius: i32,
    pub max_side_length: i32,
    pub max_expansion_side_length: i32,
    pub claim_world: BTreeMap<String, BTreeSet<Flag>>,
    pub ignore_flags: BTreeSet<Flag>,
    pub flags: Rules,
    pub world_aliases: BTreeMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enable_world: BTreeSet::from(["world".into()]),
            member_limit: 100,
            member_input_timeout: 60,
            particle: Particle::default(),
            permission_nodes: BTreeMap::new(),
            area_limit: 10,
            max_radius: 128,
            max_side_length: 256,
            max_expansion_side_length: 64,
            claim_world: BTreeMap::new(),
            ignore_flags: BTreeSet::from([Flag::Teleport]),
            flags: Rules::new(),
            world_aliases: BTreeMap::from([
                ("minecraft:overworld".into(), "world".into()),
                ("minecraft:the_nether".into(), "world_nether".into()),
                ("minecraft:the_end".into(), "world_the_end".into()),
            ]),
        }
    }
}

#[derive(Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Particle {
    pub period: u64,
    pub distance: f64,
    pub range: f64,
    pub budget: usize,
}
impl Default for Particle {
    fn default() -> Self {
        Self {
            period: 2,
            distance: 1.5,
            range: 50.0,
            budget: 500,
        }
    }
}

impl Settings {
    pub fn permission(&self, uuid: Uuid, node: &str, default: bool) -> bool {
        self.permission_nodes
            .get(&uuid)
            .and_then(|nodes| nodes.get(node))
            .copied()
            .unwrap_or(default)
    }

    pub fn default_for(&self, flag: Flag) -> bool {
        self.flags
            .get(&flag)
            .copied()
            .unwrap_or_else(|| flag.default_value())
    }

    pub fn world<'a>(&'a self, dimension: &'a str) -> &'a str {
        self.world_aliases
            .get(dimension)
            .map(String::as_str)
            .unwrap_or(dimension)
    }
}

#[derive(Clone, Deserialize)]
pub struct Item {
    #[serde(default = "default_material")]
    pub material: String,
    #[serde(default = "default_name")]
    pub name: String,
    #[serde(default)]
    pub lore: Vec<String>,
    #[serde(default)]
    pub shiny: bool,
}

#[derive(Deserialize)]
pub struct Core {
    dimensions: Option<Axes>,
    size: Option<Axes>,
    #[serde(flatten)]
    pub item: Item,
    drop: Option<Item>,
}

#[derive(Clone, Copy, Deserialize)]
struct Axes {
    x: i32,
    y: i32,
    z: i32,
}

impl Axes {
    fn array(self) -> Point {
        [self.x, self.y, self.z]
    }
}

impl Core {
    fn starter() -> Self {
        Self {
            dimensions: Some(Axes { x: 4, y: 4, z: 4 }),
            size: None,
            item: Item {
                material: default_material(),
                name: "&e初始领地核心".into(),
                lore: vec![
                    "&7保护范围：4×4×4".into(),
                    "&7放置后创建领地，右键管理或扩建".into(),
                ],
                shiny: true,
            },
            drop: None,
        }
    }

    pub fn drop_item(&self) -> std::borrow::Cow<'_, Item> {
        self.drop.as_ref().map_or_else(
            || {
                std::borrow::Cow::Owned(Item {
                    material: "GHAST_TEAR".into(),
                    name: format!("&7破碎的{}", crate::cores::plain_color(&self.item.name)),
                    lore: Vec::new(),
                    shiny: false,
                })
            },
            std::borrow::Cow::Borrowed,
        )
    }

    pub fn volume(&self, id: &str, pos: Point, settings: &Settings) -> Result<Volume, String> {
        match (self.dimensions, self.size) {
            (Some(dims), None)
                if id == STARTER_CORE || dims.array().iter().all(|v| *v <= settings.max_side_length) =>
            {
                Volume::sized(pos, dims.array(), id.into())
            }
            (None, Some(size)) if size.array().iter().all(|v| *v <= settings.max_radius) => {
                Volume::legacy(pos, size.array(), id.into())
            }
            _ => Err(format!("核心 {id} 的 dimensions/size 配置无效")),
        }
    }
}

#[derive(Deserialize, Serialize)]
struct Snapshot<V> {
    schema: u8,
    areas: BTreeMap<Uuid, StoredArea<V>>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
struct StoredArea<V> {
    world: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owner_uuid: Option<Uuid>,
    owner_name: String,
    name: Option<String>,
    welcome: Option<String>,
    farewell: Option<String>,
    volumes: Vec<V>,
    #[serde(default, skip_serializing_if = "Rules::is_empty")]
    flags: Rules,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    members: BTreeMap<String, Member>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
struct Absolute {
    x: i32,
    y: i32,
    z: i32,
    min_x: i32,
    min_y: i32,
    min_z: i32,
    max_x: i32,
    max_y: i32,
    max_z: i32,
    #[serde(default = "default_core")]
    core: String,
}

#[derive(Deserialize)]
struct Radius {
    x: i32,
    y: i32,
    z: i32,
    rx: i32,
    ry: i32,
    rz: i32,
    #[serde(default = "default_core")]
    core: String,
}

#[derive(Deserialize)]
struct LegacyArea {
    owner: String,
    size: Axes,
    #[serde(default)]
    index: i32,
    #[serde(default)]
    flags: Rules,
    #[serde(default)]
    members: BTreeMap<String, Rules>,
}

fn default_material() -> String {
    "WHITE_STAINED_GLASS".into()
}
fn default_name() -> String {
    "领地核心".into()
}

fn default_core() -> String {
    "0".into()
}

fn decode<T: serde::de::DeserializeOwned>(contents: &str) -> Result<T, String> {
    serde_yaml_ng::from_str(contents).map_err(|err| err.to_string())
}

fn decode_snapshot<V>(
    raw: Snapshot<V>,
    convert: impl Fn(V) -> Result<Volume, String>,
) -> Result<AreaIndex, String> {
    let mut index = AreaIndex::default();
    for (id, record) in raw.areas {
        let volumes = record
            .volumes
            .into_iter()
            .map(&convert)
            .collect::<Result<Vec<_>, _>>()?;
        if volumes.is_empty()
            || volumes.len() > 257
            || record.world.trim().is_empty()
            || record.owner_name.trim().is_empty()
        {
            return Err(format!("无效领地存档：{id}"));
        }
        let mut area = Area::new(
            id,
            record.world,
            record.owner_uuid,
            record.owner_name,
            volumes,
        );
        area.name = record.name.unwrap_or(area.name);
        area.welcome = record.welcome.unwrap_or(area.welcome);
        area.farewell = record.farewell.unwrap_or(area.farewell);
        area.flags = record.flags;
        area.members = record.members;
        index.add(area)?;
    }
    Ok(index)
}

pub fn parse(contents: &str) -> Result<AreaIndex, String> {
    #[derive(Deserialize)]
    struct Header {
        schema: u8,
    }

    match decode::<Header>(contents)?.schema {
        2 => decode_snapshot(decode::<Snapshot<Radius>>(contents)?, |v| {
            Volume::legacy([v.x, v.y, v.z], [v.rx, v.ry, v.rz], v.core)
        }),
        3 => decode_snapshot(decode::<Snapshot<Absolute>>(contents)?, |v| {
            Volume::new(
                [v.x, v.y, v.z],
                [v.min_x, v.min_y, v.min_z],
                [v.max_x, v.max_y, v.max_z],
                v.core,
            )
        }),
        version => Err(format!("不支持的领地数据版本：{version}")),
    }
}

#[derive(Clone)]
pub struct Store {
    directory: PathBuf,
}

impl Store {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    pub fn initialise_config(&self) -> io::Result<()> {
        let path = self.directory.join("config.yml");
        if path.try_exists()? {
            return Ok(());
        }
        fs::create_dir_all(&self.directory)?;
        let mut staged = NamedTempFile::new_in(&self.directory)?;
        staged.write_all(DEFAULT_CONFIG.as_bytes())?;
        staged.as_file().sync_all()?;
        match staged.persist_noclobber(path) {
            Ok(_) => Ok(()),
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(error.error),
        }
    }

    pub fn config(&self) -> Result<Config, String> {
        let path = self.directory.join("config.yml");
        let contents = read_optional(&path)?.unwrap_or_else(|| DEFAULT_CONFIG.into());
        let mut config: Config = decode(&contents)?;
        config.settings.max_radius = config.settings.max_radius.min(128);
        config.settings.max_side_length = config.settings.max_side_length.min(256);
        if config.cores.is_empty()
            || !(1..=128).contains(&config.settings.max_radius)
            || !(1..=256).contains(&config.settings.max_side_length)
            || !(4..=256).contains(&config.settings.max_expansion_side_length)
            || config
                .settings
                .world_aliases
                .values()
                .any(|world| world.trim().is_empty())
        {
            return Err("核心配置无效".into());
        }
        for (id, core) in &config.cores {
            if id == STARTER_CORE {
                continue;
            }
            core.volume(id, [0; 3], &config.settings)?;
            if std::iter::once(&core.item)
                .chain(&core.drop)
                .any(|item| item.material.is_empty() || item.name.is_empty())
            {
                return Err(format!("核心 {id} 的物品配置无效"));
            }
        }
        config.cores.insert(STARTER_CORE.into(), Core::starter());
        Ok(config)
    }

    pub fn load_known(&self) -> Result<KnownPlayers, String> {
        read_optional(&self.directory.join("known-players.yml"))?
            .map_or_else(|| Ok(KnownPlayers::new()), |content| decode(&content))
    }

    pub fn save_known(&self, players: &KnownPlayers) -> Result<(), String> {
        let data = serde_yaml_ng::to_string(players).map_err(|err| err.to_string())?;
        fs::create_dir_all(&self.directory).map_err(|err| err.to_string())?;
        persist(
            &self.directory,
            &self.directory.join("known-players.yml"),
            data.as_bytes(),
        )
        .map_err(|err| err.to_string())
    }

    pub fn load(&self) -> Result<AreaIndex, String> {
        let path = self.directory.join("areas.yml");
        read_optional(&path)?.map_or_else(|| Ok(AreaIndex::default()), |contents| parse(&contents))
    }

    pub fn save(&self, index: &AreaIndex) -> Result<(), String> {
        let areas = index
            .areas
            .iter()
            .map(|(id, area)| {
                let volumes = area
                    .volumes
                    .iter()
                    .map(|v| Absolute {
                        x: v.core_pos[0],
                        y: v.core_pos[1],
                        z: v.core_pos[2],
                        min_x: v.min[0],
                        min_y: v.min[1],
                        min_z: v.min[2],
                        max_x: v.max[0],
                        max_y: v.max[1],
                        max_z: v.max[2],
                        core: v.core.clone(),
                    })
                    .collect();
                (
                    *id,
                    StoredArea {
                        world: area.world.clone(),
                        owner_uuid: area.owner,
                        owner_name: area.owner_name.clone(),
                        name: Some(area.name.clone()),
                        welcome: Some(area.welcome.clone()),
                        farewell: Some(area.farewell.clone()),
                        volumes,
                        flags: area.flags.clone(),
                        members: area.members.clone(),
                    },
                )
            })
            .collect();
        let yaml =
            serde_yaml_ng::to_string(&Snapshot { schema: 3, areas }).map_err(|e| e.to_string())?;
        self.commit(yaml.as_bytes()).map_err(|e| e.to_string())
    }

    fn commit(&self, yaml: &[u8]) -> io::Result<()> {
        fs::create_dir_all(&self.directory)?;
        let path = self.directory.join("areas.yml");
        match fs::read(&path) {
            Ok(original) => persist(
                &self.directory,
                &self.directory.join("areas.yml.bak"),
                &original,
            )?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        persist(&self.directory, &path, yaml)?;
        // The replacement is already committed. Directory syncing is best effort here.
        #[cfg(unix)]
        let _ = File::open(&self.directory).and_then(|file| file.sync_all());
        Ok(())
    }

    pub fn import_legacy(
        &self,
        index: &mut AreaIndex,
        settings: &Settings,
    ) -> Result<usize, String> {
        let folder = self.directory.join("legacy-save");
        let mut paths = fs::read_dir(&folder)
            .map_err(|e| format!("{}：{e}", folder.display()))?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<io::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        paths.sort();
        let mut imported = 0;
        for path in paths
            .iter()
            .filter(|p| p.extension().is_some_and(|ext| ext == "yml"))
        {
            let world = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("无效旧世界名称")?;
            let records: BTreeMap<String, LegacyArea> =
                decode(&fs::read_to_string(path).map_err(|e| e.to_string())?)?;
            for (location, record) in records {
                let id = legacy_id(world, &location);
                if index.areas.contains_key(&id) {
                    continue;
                }
                let pos = location
                    .split(',')
                    .map(str::parse::<i32>)
                    .collect::<Result<Vec<_>, _>>()
                    .ok()
                    .and_then(|parts| parts.try_into().ok())
                    .ok_or_else(|| format!("无效旧坐标：{location}"))?;
                if record.owner.trim().is_empty() {
                    return Err(format!("无效旧领地主人：{}", record.owner));
                }
                let volume = Volume::legacy(pos, record.size.array(), record.index.to_string())?;
                let mut area = Area::new(id, world.into(), None, record.owner, vec![volume]);
                area.flags = record.flags;
                for &flag in Flag::ALL {
                    area.flags
                        .entry(flag)
                        .or_insert_with(|| settings.default_for(flag));
                }
                for (name, flags) in record.members {
                    area.members
                        .insert(name.to_ascii_lowercase(), Member { name, flags });
                }
                index.add(area)?;
                imported += 1;
            }
        }
        Ok(imported)
    }
}

fn legacy_id(world: &str, pos: &str) -> Uuid {
    let mut digest = md5::compute(format!("{world}:{pos}")).0;
    digest[6] = digest[6] & 0x0f | 0x30;
    digest[8] = digest[8] & 0x3f | 0x80;
    Uuid::from_bytes(digest)
}

fn read_optional(path: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}：{e}", path.display())),
    }
}

fn persist(directory: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut staging = NamedTempFile::new_in(directory)?;
    staging.write_all(bytes)?;
    staging.as_file().sync_all()?;
    staging.persist(path).map_err(|e| e.error)?;
    Ok(())
}

pub fn known_from_world_cache(instance: &Path, world: &Path) -> Result<KnownPlayers, String> {
    #[derive(Deserialize)]
    struct Profile {
        name: String,
        uuid: Uuid,
    }
    let Some(cache) = read_optional(&instance.join("usercache.json"))? else {
        return Ok(KnownPlayers::new());
    };
    let mut known = KnownPlayers::new();
    for Profile { name, uuid } in decode::<Vec<Profile>>(&cache)? {
        if !player_name(&name) {
            continue;
        }
        let file = format!("{uuid}.dat");
        if world.join("players/data").join(&file).is_file()
            || world.join("playerdata").join(&file).is_file()
        {
            known
                .entry(name.to_ascii_lowercase())
                .or_insert((uuid, name));
        }
    }
    Ok(known)
}
