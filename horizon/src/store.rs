use crate::model::{Area, AreaIndex, Flag, Member, Point, Rules, Volume, player_name};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use uuid::Uuid;

const DEFAULT_CONFIG: &str = include_str!("../data/purtmarsarea/config.yml");

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
    pub area_limit: usize,
    pub max_radius: i32,
    pub max_side_length: i32,
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
            area_limit: 10,
            max_radius: 128,
            max_side_length: 256,
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

impl Settings {
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

#[derive(Deserialize)]
pub struct Item {
    pub material: String,
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
    pub drop: Item,
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
    pub fn volume(&self, id: &str, pos: Point, settings: &Settings) -> Result<Volume, String> {
        match (self.dimensions, self.size) {
            (Some(dims), None) if dims.array().iter().all(|v| *v <= settings.max_side_length) => {
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
            || !player_name(&record.owner_name)
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

    pub fn config(&self) -> Result<Config, String> {
        let path = self.directory.join("config.yml");
        let contents = read_optional(&path)?.unwrap_or_else(|| DEFAULT_CONFIG.into());
        let config: Config = decode(&contents)?;
        if config.cores.is_empty()
            || !(1..=128).contains(&config.settings.max_radius)
            || !(1..=256).contains(&config.settings.max_side_length)
            || config
                .settings
                .world_aliases
                .values()
                .any(|world| world.trim().is_empty())
        {
            return Err("核心配置无效".into());
        }
        for (id, core) in &config.cores {
            core.volume(id, [0; 3], &config.settings)?;
            if [&core.item, &core.drop]
                .iter()
                .any(|item| item.material.is_empty() || item.name.is_empty())
            {
                return Err(format!("核心 {id} 的物品配置无效"));
            }
        }
        Ok(config)
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
                if !player_name(&record.owner) {
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
                    if !player_name(&name) {
                        return Err(format!("无效旧成员：{name}"));
                    }
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
