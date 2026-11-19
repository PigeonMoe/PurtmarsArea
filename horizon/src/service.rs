use crate::{
    model::{Area, AreaIndex, Flag, Member, Point, player_name},
    store::{Config, KnownPlayers, Settings, Store},
};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    path::PathBuf,
    time::{Duration, Instant},
};
use uuid::Uuid;

pub struct Actor {
    pub uuid: Uuid,
    pub name: String,
    pub operator: bool,
}

impl Actor {
    pub fn admin(&self, settings: &Settings) -> bool {
        settings.permission(self.uuid, "PurtmarsArea.command.admin", self.operator)
    }

    pub fn manages(&self, settings: &Settings, area: &Area) -> bool {
        self.admin(settings) || area.is_owner(self.uuid, &self.name)
    }
}

#[derive(Default)]
pub struct Service {
    pub store: Option<Store>,
    pub config: Option<Config>,
    pub index: AreaIndex,
    pub error: Option<String>,
    pub denials: BTreeMap<Flag, u64>,
    pub explosions: VecDeque<String>,
    pub known_players: KnownPlayers,
    known_player_storage_ok: bool,
    pub clicked_cores: HashMap<Uuid, String>,
    pub pending_placements: HashMap<Uuid, Area>,
    pub slimefun_loaded: bool,
    pub menus: HashMap<Uuid, crate::menu::Menu>,
    pub member_inputs: HashMap<Uuid, crate::menu::MemberInput>,
    pub ticks: u64,
    pub transit_entities: HashMap<u128, u64>,
    pub pending_notices: Vec<(Uuid, String)>,
    pub denied_shots: HashSet<crate::combat::Shot>,
    notices: HashMap<Uuid, Instant>,
    occupations: HashMap<Uuid, (Uuid, Instant)>,
}

impl Service {
    pub fn start(&mut self, path: PathBuf) {
        *self = Self {
            store: Some(Store::new(path)),
            ..Self::default()
        };
        if let Some(store) = &self.store {
            if let Err(error) = store.initialise_config() {
                self.error = Some(error.to_string());
                return;
            }
            match store.load_known() {
                Ok(players) => {
                    self.known_players = players;
                    self.known_player_storage_ok = true;
                }
                Err(error) => eprintln!("[PurtmarsArea] 无法载入玩家记录：{error}"),
            }
        }
        let _ = self.reload();
    }

    pub fn seed_known(
        &mut self,
        instance: &std::path::Path,
        world: &std::path::Path,
    ) -> Result<(), String> {
        let known = crate::store::known_from_world_cache(instance, world)?;
        let mut added = false;
        for (key, identity) in known {
            if let std::collections::btree_map::Entry::Vacant(entry) = self.known_players.entry(key)
            {
                entry.insert(identity);
                added = true;
            }
        }
        if added
            && self.known_player_storage_ok
            && let Some(store) = &self.store
        {
            store.save_known(&self.known_players)?;
        }
        Ok(())
    }

    pub fn reload(&mut self) -> Result<(), String> {
        self.member_inputs.clear();
        self.occupations.clear();
        self.menus.clear();
        let store = self.store.as_ref().ok_or("领地插件尚未加载")?;
        match store
            .config()
            .and_then(|config| store.load().map(|index| (config, index)))
        {
            Ok((config, index)) => {
                self.index = index;
                self.config = Some(config);
                self.error = None;
                Ok(())
            }
            Err(error) => {
                self.error = Some(error.clone());
                Err(error)
            }
        }
    }

    pub fn ready(&self) -> Result<&Config, String> {
        if self.error.is_some() {
            return Err("领地系统已锁定，请联系管理员修复配置或数据".into());
        }
        self.config
            .as_ref()
            .ok_or_else(|| "领地插件尚未加载".into())
    }

    pub fn world<'a>(&'a self, dimension: &'a str) -> &'a str {
        self.config
            .as_ref()
            .map_or(dimension, |c| c.settings.world(dimension))
    }

    pub fn at(&self, dimension: &str, pos: Point) -> Option<&Area> {
        self.index.at(self.world(dimension), pos)
    }

    pub fn core(&self, dimension: &str, pos: Point) -> Option<&Area> {
        self.at(dimension, pos)
            .filter(|area| area.core(pos).is_some())
    }

    pub fn allowed(&self, actor: &Actor, dimension: &str, pos: Point, flag: Flag) -> bool {
        let Ok(config) = self.ready() else {
            return false;
        };
        if config
            .settings
            .permission(actor.uuid, "PurtmarsArea.bypass", actor.operator)
        {
            return true;
        }
        self.at(dimension, pos).map_or_else(
            || {
                !config
                    .settings
                    .claim_world
                    .get(self.world(dimension))
                    .is_some_and(|flags| flags.contains(&flag))
            },
            |area| {
                area.allows(
                    flag,
                    actor.uuid,
                    &actor.name,
                    config.settings.default_for(flag),
                )
            },
        )
    }

    pub fn allows_device_build(
        &self,
        uuid: Uuid,
        online_name: Option<&str>,
        operator: bool,
        dimension: &str,
        pos: Point,
    ) -> bool {
        if self.store.is_none() {
            return true;
        }
        let name = online_name.or_else(|| {
            self.known_players
                .values()
                .find(|(known, _)| *known == uuid)
                .map(|(_, name)| name.as_str())
        });
        let actor = Actor {
            uuid,
            name: name.unwrap_or_default().to_owned(),
            operator,
        };
        self.core(dimension, pos).is_none() && self.allowed(&actor, dimension, pos, Flag::Build)
    }

    pub fn natural(&self, dimension: &str, pos: Point, flag: Flag) -> bool {
        let Ok(config) = self.ready() else {
            return false;
        };
        self.at(dimension, pos)
            .is_none_or(|area| area.enabled(flag, config.settings.default_for(flag)))
    }

    pub fn crosses(&self, dimension: &str, from: Point, to: Point) -> bool {
        self.at(dimension, from).map(|area| area.id) != self.at(dimension, to).map(|area| area.id)
    }

    pub fn blocks_flow(&self, dimension: &str, from: Point, to: Point, flag: Flag) -> bool {
        self.store.is_some()
            && (self.core(dimension, to).is_some()
                || !self.natural(dimension, from, flag)
                || !self.natural(dimension, to, flag))
    }

    pub fn blocks_ridden_vehicle(
        &mut self,
        dimension: &str,
        from: Point,
        to: Point,
        player: impl FnOnce() -> Option<Actor>,
    ) -> bool {
        if self.store.is_none() || !self.crosses(dimension, from, to) {
            return false;
        }
        if let Some(actor) =
            player().filter(|actor| !self.allowed(actor, dimension, to, Flag::Move))
        {
            self.queue_denial(actor.uuid, Flag::Move);
            return true;
        }
        false
    }

    pub fn blocks_piston(
        &self,
        dimension: &str,
        origin: Point,
        direction: Point,
        mut affected: impl Iterator<Item = Point>,
    ) -> bool {
        if self.store.is_none() {
            return false;
        }
        let within = |pos| {
            !self.crosses(dimension, origin, pos)
                && self.natural(dimension, pos, Flag::RedstonePerform)
                && self.core(dimension, pos).is_none()
        };
        let step = |pos: Point| std::array::from_fn(|i| pos[i].saturating_add(direction[i]));
        !within(origin)
            || !within(step(origin))
            || affected.any(|pos| !within(pos) || !within(step(pos)))
    }

    pub fn blocks_physical_trigger(
        &mut self,
        dimension: &str,
        pos: Point,
        player: Option<&Actor>,
        flag: Flag,
    ) -> bool {
        if self.store.is_none() {
            return false;
        }
        if flag == Flag::Build && self.core(dimension, pos).is_some() {
            return true;
        }
        let Some(actor) = player else {
            return !self.natural(dimension, pos, Flag::Build);
        };
        let denied = !self.allowed(actor, dimension, pos, flag);
        if denied {
            self.queue_denial(actor.uuid, flag);
        }
        denied
    }

    pub fn blocks_falling(
        &self,
        dimension: &str,
        target: Point,
        origin: Option<(&str, Point)>,
    ) -> bool {
        let claim = self.at(dimension, target);
        self.store.is_some()
            && (claim.is_some_and(|area| area.core(target).is_some())
                || (origin.is_none_or(|(origin_dimension, origin)| {
                    self.at(origin_dimension, origin).map(|area| area.id)
                        != claim.map(|area| area.id)
                }) && !self.natural(dimension, target, Flag::Falling)))
    }

    pub fn blocks_growth(
        &mut self,
        dimension: &str,
        origin: Point,
        target: Point,
        structure: bool,
        fertilized: bool,
        player: Option<&Actor>,
    ) -> bool {
        if self.store.is_none() {
            return false;
        }
        if self.core(dimension, target).is_some()
            || (structure
                && self.crosses(dimension, origin, target)
                && !self.natural(dimension, target, Flag::Build))
        {
            return true;
        }
        if !fertilized {
            return false;
        }
        match player {
            Some(actor) => {
                let denied = !self.allowed(actor, dimension, target, Flag::Build);
                if denied {
                    self.queue_denial(actor.uuid, Flag::Build);
                }
                denied
            }
            None => !self.natural(dimension, target, Flag::Build),
        }
    }

    pub fn denial(&mut self, actor: Uuid, flag: Flag) -> Option<String> {
        let count = self.denials.entry(flag).or_default();
        *count = count.saturating_add(1);
        let now = Instant::now();
        if self
            .notices
            .get(&actor)
            .is_some_and(|last| now.duration_since(*last) < Duration::from_secs(1))
        {
            return None;
        }
        self.notices.insert(actor, now);
        Some(if self.error.is_some() {
            "领地系统已锁定，请联系管理员".into()
        } else {
            format!("此处不允许：{}", flag.title())
        })
    }

    pub fn queue_denial(&mut self, player: Uuid, flag: Flag) {
        if let Some(notice) = self.denial(player, flag) {
            self.pending_notices.push((player, notice));
        }
    }

    pub fn occupation(&mut self, player: Uuid, area: Uuid) -> Option<String> {
        let now = Instant::now();
        if self.occupations.get(&player).is_some_and(|(id, last)| {
            *id == area && now.duration_since(*last) < Duration::from_secs(2)
        }) {
            return None;
        }
        let owner = self.index.areas.get(&area)?.owner_name.clone();
        self.occupations.insert(player, (area, now));
        Some(owner)
    }

    pub fn explosion(&mut self, dimension: &str, center: [f64; 3], blocks: &[Point]) -> bool {
        let (hits, first) = blocks.iter().fold((0, None), |(hits, first), &p| {
            if self.core(dimension, p).is_some() || !self.natural(dimension, p, Flag::Explosion) {
                (hits + 1, first.or(Some(p)))
            } else {
                (hits, first)
            }
        });
        let [x, y, z] = center;
        let mut line = format!(
            "爆炸 | {dimension} {x:.2},{y:.2},{z:.2} | 候选方块 {} | 受保护 {hits}",
            blocks.len()
        );
        if let Some([x, y, z]) = first {
            line.push_str(&format!(" | 已取消整批方块破坏 | 首个命中 {x},{y},{z}"));
        } else {
            line.push_str(" | 未命中保护范围");
        }
        self.explosions.truncate(9);
        self.explosions.push_front(line);
        hits > 0
    }

    pub fn save(&self) -> Result<(), String> {
        self.ready()?;
        self.store
            .as_ref()
            .ok_or("领地插件尚未加载")?
            .save(&self.index)
    }

    pub fn change<T>(
        &mut self,
        edit: impl FnOnce(&mut AreaIndex, &Config) -> Result<T, String>,
    ) -> Result<T, String> {
        let config = self.ready()?;
        let mut candidate = self.index.clone();
        let result = edit(&mut candidate, config)?;
        self.store
            .as_ref()
            .ok_or("领地插件尚未加载")?
            .save(&candidate)?;
        self.index = candidate;
        Ok(result)
    }

    pub fn validate_create(
        &self,
        actor: &Actor,
        dimension: &str,
        pos: Point,
        id: &str,
        height: std::ops::Range<i32>,
    ) -> Result<Area, String> {
        let config = self.ready()?;
        let world = self.world(dimension);
        if !config
            .settings
            .permission(actor.uuid, "PurtmarsArea.create", true)
        {
            return Err("没有创建领地的权限".into());
        }
        if !config.settings.enable_world.contains(world)
            && !config.settings.permission(
                actor.uuid,
                &format!("PurtmarsArea.bypass.{world}"),
                false,
            )
        {
            return Err("该世界不允许放置领地核心".into());
        }
        let core = config.cores.get(id).ok_or("未知核心编号")?;
        let volume = core.volume(id, pos, &config.settings)?;
        if !height.contains(&volume.min[1]) || !height.contains(&volume.max[1]) {
            return Err("核心保护范围超出世界高度".into());
        }
        if self.index.overlaps(world, &volume) {
            return Err("核心保护范围不能与任何已有核心重叠，包括自己的领地".into());
        }
        let owned = self
            .index
            .areas
            .values()
            .filter(|a| a.is_owner(actor.uuid, &actor.name))
            .count();
        if !actor.admin(&config.settings) && owned >= config.settings.area_limit {
            return Err("领地数量达到上限".into());
        }
        let mut area = Area::new(
            Uuid::new_v4(),
            world.into(),
            Some(actor.uuid),
            actor.name.clone(),
            vec![volume],
        );
        area.flags.extend(
            Flag::ALL
                .iter()
                .map(|&flag| (flag, config.settings.default_for(flag))),
        );
        Ok(area)
    }

    pub fn remove_core(&mut self, area: Uuid, pos: Point) -> Result<(), String> {
        self.change(|index, _| {
            let mut area = index.remove(area).ok_or("领地已发生变化")?;
            let volume = area
                .volumes
                .iter()
                .position(|volume| volume.core_pos == pos)
                .ok_or("领地核心已发生变化")?;
            if volume == 0 && area.volumes.len() > 1 {
                return Err("请先拆除全部扩展核心".into());
            }
            area.volumes.remove(volume);
            if !area.volumes.is_empty() {
                index.add(area)?;
            }
            Ok(())
        })
    }

    pub fn import_legacy(&mut self, online: &[(Uuid, String)]) -> Result<usize, String> {
        let store = self.store.clone().ok_or("领地插件尚未加载")?;
        self.change(|index, config| {
            let count = store.import_legacy(index, &config.settings)?;
            for (uuid, name) in online {
                bind_legacy(index, *uuid, name);
            }
            Ok(count)
        })
    }

    pub fn remember(&mut self, uuid: Uuid, name: &str) -> Result<(), String> {
        if self.store.is_none() || !player_name(name) {
            return Ok(());
        }
        let key = name.to_ascii_lowercase();
        let identity = (uuid, name.to_owned());
        if self.known_players.get(&key) != Some(&identity) {
            self.known_players
                .retain(|entry, (id, _)| *id != uuid || entry == &key);
            self.known_players.insert(key.clone(), identity);
            if self.known_player_storage_ok
                && let Some(store) = &self.store
                && let Err(error) = store.save_known(&self.known_players)
            {
                eprintln!("[PurtmarsArea] 无法保存玩家记录：{error}");
            }
        }
        if self.error.is_none()
            && self.index.areas.values().any(|area| {
                (area.owner.is_none() && area.owner_name.eq_ignore_ascii_case(name))
                    || area.members.contains_key(&key)
            })
        {
            self.change(|index, _| {
                bind_legacy(index, uuid, name);
                Ok(())
            })?;
        }
        Ok(())
    }

    pub fn quit(&mut self, uuid: Uuid) {
        self.notices.remove(&uuid);
        self.occupations.remove(&uuid);
        self.clicked_cores.remove(&uuid);
        self.pending_placements.remove(&uuid);
        self.menus.remove(&uuid);
        self.member_inputs.remove(&uuid);
    }

    pub fn member_key(area: &Area, name: &str) -> Result<String, String> {
        area.members
            .iter()
            .find(|(_, member)| member.name.eq_ignore_ascii_case(name))
            .map(|(key, _)| key.clone())
            .ok_or_else(|| "该玩家不是领地成员".into())
    }

    pub fn add_member(&mut self, area_id: Uuid, actor: &Actor, name: &str) -> Result<(), String> {
        if !player_name(name) {
            return Err("请输入有效的玩家名称".into());
        }
        let (uuid, name) = self
            .known_players
            .get(&name.to_ascii_lowercase())
            .cloned()
            .ok_or("玩家必须曾经加入此服务器")?;
        self.change(|index, config| {
            let area = manager_area(index, area_id, actor, &config.settings)?;
            if area.is_owner(uuid, &name) || area.member(uuid, &name).is_some() {
                return Err("该玩家已是领地主人或成员".into());
            }
            if area.members.len() >= config.settings.member_limit {
                return Err("成员达到上限".into());
            }
            area.members.insert(
                uuid.to_string(),
                Member {
                    name,
                    flags: BTreeMap::new(),
                },
            );
            Ok(())
        })
    }
}

pub fn manager_area<'a>(
    index: &'a mut AreaIndex,
    id: Uuid,
    actor: &Actor,
    settings: &Settings,
) -> Result<&'a mut Area, String> {
    let area = index
        .areas
        .get_mut(&id)
        .ok_or("领地已发生变化，请重新打开菜单")?;
    if actor.manages(settings, area) {
        Ok(area)
    } else {
        Err("只有领地主人或管理员可以管理".into())
    }
}

fn bind_legacy(index: &mut AreaIndex, uuid: Uuid, name: &str) {
    let name_key = name.to_ascii_lowercase();
    let uuid_key = uuid.to_string();
    for area in index.areas.values_mut() {
        if area.owner.is_none() && area.owner_name.eq_ignore_ascii_case(name) {
            area.owner = Some(uuid);
        }
        if let Some(member) = area.members.remove(&name_key) {
            area.members.entry(uuid_key.clone()).or_insert(member);
        }
    }
}
