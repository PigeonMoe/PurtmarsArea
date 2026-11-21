use crate::{
    Mod, actor, cores,
    model::{ExpansionDirection, Flag, Point},
    service::{Actor, Expansion, manager_area},
    tell,
};
use mod_api::{
    CommandSource, HostTransaction, Identifier, ItemPlace, ItemResource, ItemStack, MenuClickedEvent,
    MenuReplace, PlayerRef, ServerHost, Tag, Text,
};
use std::collections::BTreeMap;
use uuid::Uuid;

const FLAG_SLOTS: [usize; 22] = [
    10, 11, 12, 13, 14, 15, 16, 19, 20, 21, 22, 23, 24, 25, 28, 29, 30, 31, 32, 33, 34, 37,
];
const MEMBER_SLOTS: [usize; 28] = [
    10, 11, 12, 13, 14, 15, 16, 19, 20, 21, 22, 23, 24, 25, 28, 29, 30, 31, 32, 33, 34, 37, 38, 39,
    40, 41, 42, 43,
];

#[derive(Clone)]
pub enum Page {
    List(usize),
    Home(Uuid, Option<Point>),
    Expansion(Uuid, Point),
    Rules {
        area: Uuid,
        member: Option<String>,
        page: usize,
    },
    Members(Uuid, usize),
}

#[derive(Clone)]
enum Action {
    Open(Page),
    Home(Uuid),
    Rule {
        area: Uuid,
        member: Option<String>,
        flag: Flag,
        page: usize,
    },
    Add(Uuid),
    Remove(Uuid, String),
    Expand(Expansion),
}

pub struct Menu {
    pub window: i32,
    actions: BTreeMap<i32, Action>,
}

struct View {
    title: &'static str,
    items: Vec<Option<ItemStack>>,
    actions: BTreeMap<i32, Action>,
}

impl View {
    fn new(title: &'static str, rows: usize) -> Self {
        Self {
            title,
            items: vec![None; rows * 9],
            actions: BTreeMap::new(),
        }
    }

    fn button(&mut self, slot: usize, stack: Option<ItemStack>, action: Action) {
        if stack.is_some() {
            self.items[slot] = stack;
            self.actions.insert(slot as i32, action);
        }
    }

    fn pages(
        &mut self,
        host: &dyn ServerHost,
        page: usize,
        count: usize,
        size: usize,
        link: impl Fn(usize) -> Page,
    ) {
        if page > 0 {
            self.button(
                45,
                icon(host, "arrow", "&f上一页", &[]),
                Action::Open(link(page - 1)),
            );
        }
        if (page + 1) * size < count {
            self.button(
                53,
                icon(host, "arrow", "&f下一页", &[]),
                Action::Open(link(page + 1)),
            );
        }
    }
}

pub fn at_player(
    modification: &Mod,
    host: &mut dyn ServerHost,
    source: &CommandSource,
    list: bool,
) -> Result<(), String> {
    let CommandSource::Player { player, .. } = *source else {
        return Err("此命令需要玩家执行".into());
    };
    if list {
        open(modification, host, player, Page::List(0))
    } else {
        open_current(modification, host, player)
    }
}

pub fn open_current(
    modification: &Mod,
    host: &mut dyn ServerHost,
    player: PlayerRef,
) -> Result<(), String> {
    let (dimension, pos) = location(host, player)?;
    let area = modification
        .service()
        .at(&dimension, pos)
        .map(|area| area.id)
        .ok_or("当前位置没有领地")?;
    open(modification, host, player, Page::Home(area, None))
}

pub fn open(
    modification: &Mod,
    host: &mut dyn ServerHost,
    player: PlayerRef,
    page: Page,
) -> Result<(), String> {
    let actor = actor(host, player).ok_or("无法读取玩家身份")?;
    let view = {
        let service = modification.service();
        let config = service.ready()?;
        let area = |id| {
            let area = service
                .index
                .areas
                .get(&id)
                .ok_or("领地已发生变化，请重新打开菜单")?;
            if !actor.manages(&config.settings, area) {
                return Err("只有领地主人或管理员可以管理");
            }
            Ok(area)
        };
        match page {
            Page::List(page) => {
                let mut view = View::new("领地列表", 6);
                let areas: Vec<_> = service
                    .index
                    .areas
                    .values()
                    .filter(|area| actor.manages(&config.settings, area))
                    .collect();
                let page = clamp_page(page, areas.len(), 45);
                for (slot, area) in areas.iter().skip(page * 45).take(45).enumerate() {
                    let [x, y, z] = area.volumes[0].core_pos;
                    let owner = format!("&7主人：{}", area.owner_name);
                    let coordinates = format!("&7{} {x}, {y}, {z}", area.world);
                    let label = format!("&e{}", area.name);
                    view.button(
                        slot,
                        icon(host, "beacon", &label, &[&owner, &coordinates]),
                        Action::Open(Page::Home(area.id, None)),
                    );
                }
                view.pages(host, page, areas.len(), 45, Page::List);
                view
            }
            Page::Home(id, selected) => {
                let area = area(id)?;
                let core = match selected {
                    Some(pos) => area.core(pos).ok_or("领地核心已发生变化")?,
                    None => &area.volumes[0],
                };
                let mut view = View::new("领地核心", 1);
                view.button(
                    4,
                    icon(
                        host,
                        "book",
                        "&f领地核心",
                        &["&7左键: 领地设置", "&7右键: 成员管理"],
                    ),
                    Action::Home(id),
                );
                view.button(
                    6,
                    icon(
                        host,
                        "smithing_table",
                        "&e扩建领地",
                        &["&7使用领地升级套装向指定方向扩建"],
                    ),
                    Action::Open(Page::Expansion(id, core.core_pos)),
                );
                view
            }
            Page::Expansion(id, core_pos) => {
                use ExpansionDirection::{Down, East, North, South, Up, West};
                let core = area(id)?.core(core_pos).ok_or("领地核心已发生变化")?;
                let (dimension, height) = expansion_context(host, player)?;
                let current = dimensions(core.sides());
                let mut view = View::new("领地核心 > 扩建", 3);
                let description = format!("&7当前尺寸：{current}");
                view.items[4] = icon(host, "white_stained_glass", "&f领地范围", &[&description]);
                for (slot, direction) in [
                    (10, West), (11, East), (12, Down), (14, Up), (15, North), (16, South),
                ] {
                    let selection = Expansion {
                        area: id,
                        core: core_pos,
                        direction,
                    };
                    let title = format!("&e{}", direction.title());
                    match service.preview_expansion(&actor, &dimension, height.clone(), selection) {
                        Ok(target) => {
                            let target = format!("&7{current} → {}", dimensions(target.sides()));
                            view.button(
                                slot,
                                icon(
                                    host,
                                    "green_stained_glass_pane",
                                    &title,
                                    &[&target, "&7消耗 1 个领地升级套装"],
                                ),
                                Action::Expand(selection),
                            );
                        }
                        Err(error) => {
                            let reason = format!("&c{error}");
                            view.items[slot] = icon(host, "barrier", &title, &[&reason]);
                        }
                    }
                }
                view.button(
                    22,
                    icon(host, "arrow", "&f返回", &[]),
                    Action::Open(Page::Home(id, Some(core_pos))),
                );
                view
            }
            Page::Rules {
                area: id,
                member,
                page,
            } => {
                let area = area(id)?;
                let personal = member
                    .as_ref()
                    .map(|key| area.members.get(key).ok_or("成员已被删除"))
                    .transpose()?;
                let mut view = View::new(
                    if personal.is_some() {
                        "领地核心 > 成员权限"
                    } else {
                        "领地核心 > 领地设置"
                    },
                    6,
                );
                let flags: Vec<_> = Flag::ALL
                    .iter()
                    .copied()
                    .filter(|flag| {
                        (personal.is_none() || flag.personal())
                            && !config.settings.ignore_flags.contains(flag)
                    })
                    .collect();
                let page = clamp_page(page, flags.len(), FLAG_SLOTS.len());
                for (&slot, flag) in FLAG_SLOTS
                    .iter()
                    .zip(flags.iter().copied().skip(page * FLAG_SLOTS.len()))
                {
                    let default = config.settings.default_for(flag);
                    let allowed = personal.map_or(area.enabled(flag, default), |m| {
                        m.flags.get(&flag).copied().unwrap_or(default)
                    });
                    let (material, value) = if allowed {
                        ("green_terracotta", "&a允许")
                    } else {
                        ("red_terracotta", "&c禁止")
                    };
                    let title = format!("&f{}", flag.title());
                    let description = format!("&7{}", flag.key());
                    view.button(
                        slot,
                        icon(
                            host,
                            material,
                            &title,
                            &[&description, value, "&7左键切换，右键恢复默认"],
                        ),
                        Action::Rule {
                            area: id,
                            member: member.clone(),
                            flag,
                            page,
                        },
                    );
                }
                let back = if personal.is_some() {
                    Page::Members(id, 0)
                } else {
                    Page::Home(id, None)
                };
                view.button(
                    49,
                    icon(host, "redstone_block", "&f返回", &[]),
                    Action::Open(back),
                );
                if let Some(key) = &member {
                    view.button(
                        48,
                        icon(host, "barrier", "&c移除成员", &["&7Shift + 点击确认"]),
                        Action::Remove(id, key.clone()),
                    );
                }
                view.pages(host, page, flags.len(), FLAG_SLOTS.len(), |page| {
                    Page::Rules {
                        area: id,
                        member: member.clone(),
                        page,
                    }
                });
                view
            }
            Page::Members(id, page) => {
                let area = area(id)?;
                let mut view = View::new("领地核心 > 成员管理", 6);
                let page = clamp_page(page, area.members.len(), MEMBER_SLOTS.len());
                for (&slot, (key, member)) in MEMBER_SLOTS
                    .iter()
                    .zip(area.members.iter().skip(page * MEMBER_SLOTS.len()))
                {
                    let label = format!("&f{}", member.name);
                    let head = icon(host, "player_head", &label, &["&7点击修改独立权限"]).map(
                        |mut head| {
                            let profile = Tag::Compound(
                                [("name".into(), Tag::String(member.name.as_str().into()))].into(),
                            );
                            let _ = head
                                .apply_components(&[("minecraft:profile".into(), profile)].into());
                            head
                        },
                    );
                    view.button(
                        slot,
                        head,
                        Action::Open(Page::Rules {
                            area: id,
                            member: Some(key.clone()),
                            page: 0,
                        }),
                    );
                }
                view.button(
                    49,
                    icon(host, "oak_sign", "&f添加成员", &["&7点击添加领地成员"]),
                    Action::Add(id),
                );
                view.button(
                    48,
                    icon(host, "redstone_block", "&f返回", &[]),
                    Action::Open(Page::Home(id, None)),
                );
                view.pages(host, page, area.members.len(), MEMBER_SLOTS.len(), |page| {
                    Page::Members(id, page)
                });
                view
            }
        }
    };
    let window = host
        .open_menu(
            player.uuid(),
            (view.items.len() / 9) as u8,
            Text::literal(view.title),
            view.items,
            MenuReplace::InPlace,
        )
        .map_err(|e| format!("无法打开菜单：{e:?}"))?
        .ok_or("菜单打开被取消")?;
    modification.service().menus.insert(
        actor.uuid,
        Menu {
            window,
            actions: view.actions,
        },
    );
    Ok(())
}

pub fn click(modification: &Mod, host: &mut dyn ServerHost, event: &MenuClickedEvent) {
    if !matches!(event.mode, 0 | 1) || !matches!(event.button, 0 | 1) {
        return;
    }
    let uuid = Uuid::from_u128(event.uuid);
    let action = modification
        .service()
        .menus
        .get(&uuid)
        .filter(|menu| menu.window == event.window)
        .and_then(|menu| menu.actions.get(&event.slot))
        .cloned();
    let Some((action, player)) = action.zip(host.player(event.uuid)) else {
        return;
    };
    let result = perform(modification, host, player, event, action);
    if let Err(error) = result {
        host.close_menu(event.uuid);
        tell(host, player, error);
    }
}

fn perform(
    modification: &Mod,
    host: &mut dyn ServerHost,
    player: PlayerRef,
    event: &MenuClickedEvent,
    action: Action,
) -> Result<(), String> {
    let actor = actor(host, player).ok_or("无法读取玩家身份")?;
    let next = match action {
        Action::Open(page) => page,
        Action::Home(id) if event.button == 0 => Page::Rules {
            area: id,
            member: None,
            page: 0,
        },
        Action::Home(id) => Page::Members(id, 0),
        Action::Rule {
            area,
            member,
            flag,
            page,
        } => {
            modification.service().change(|index, config| {
                if config.settings.ignore_flags.contains(&flag)
                    || (member.is_some() && !flag.personal())
                {
                    return Err("管理员已锁定该权限".into());
                }
                let area = manager_area(index, area, &actor, &config.settings)?;
                let default = config.settings.default_for(flag);
                let values = if let Some(key) = &member {
                    &mut area.members.get_mut(key).ok_or("成员已被删除")?.flags
                } else {
                    &mut area.flags
                };
                if event.button == 1 {
                    if member.is_some() {
                        values.remove(&flag);
                    } else {
                        values.insert(flag, default);
                    }
                } else {
                    values.insert(flag, !values.get(&flag).copied().unwrap_or(default));
                }
                Ok(())
            })?;
            Page::Rules { area, member, page }
        }
        Action::Add(id) => {
            begin_member_input(modification, host, player, id)?;
            return Ok(());
        }
        Action::Remove(_, _) if event.mode != 1 => return Ok(()),
        Action::Remove(id, key) => {
            modification.service().change(|index, config| {
                let area = manager_area(index, id, &actor, &config.settings)?;
                area.members.remove(&key).ok_or("成员已被删除")?;
                Ok(())
            })?;
            Page::Members(id, 0)
        }
        Action::Expand(_) if event.button != 0 || event.mode != 0 => return Ok(()),
        Action::Expand(selection) => {
            let result = expansion_context(host, player).and_then(|(dimension, height)| {
                apply_expansion(modification, host, &actor, (&dimension, height), selection)
            });
            tell(host, player, result.unwrap_or_else(|error| error));
            Page::Expansion(selection.area, selection.core)
        }
    };
    open(modification, host, player, next)
}

fn dimensions([x, y, z]: Point) -> String {
    format!("{x}×{y}×{z}")
}

fn expansion_context(
    host: &dyn ServerHost,
    player: PlayerRef,
) -> Result<(String, std::ops::Range<i32>), String> {
    let dimension = player
        .dimension(host)
        .ok_or("无法读取当前维度")?
        .to_string();
    let shape = player
        .level(host)
        .and_then(|level| level.dimension_shape(host))
        .ok_or("无法读取当前维度的建筑高度")?;
    Ok((dimension, shape.min_y..shape.min_y.saturating_add(shape.height)))
}

fn apply_expansion(
    modification: &Mod,
    host: &mut dyn ServerHost,
    actor: &Actor,
    (dimension, height): (&str, std::ops::Range<i32>),
    selection: Expansion,
) -> Result<String, String> {
    let (place, kit) = (0..36)
        .chain([-106])
        .find_map(|slot| {
            let place = ItemPlace::PlayerSlot {
                player: actor.uuid.as_u128(),
                slot,
            };
            let stack = place.stack(host)?;
            cores::is_upgrade_kit(&stack).then(|| (place, ItemResource::of(&stack)))
        })
        .ok_or("背包中需要 1 个领地升级套装")?;
    let volume = host.transaction(|host, tx| {
        if place.extract(host, tx, &kit, 1) != 1 {
            return Err("无法取用领地升级套装".into());
        }
        // A blocked boundary or failed save aborts the inventory transaction.
        modification.service().expand(actor, dimension, height, selection)
    })?;
    Ok(format!("领地已扩建至 {}", dimensions(volume.sides())))
}

pub struct MemberInput {
    area: Uuid,
    standing_in: Option<Uuid>,
    dimension: String,
    expires: u64,
    input: Option<String>,
}

fn begin_member_input(
    modification: &Mod,
    host: &mut dyn ServerHost,
    player: PlayerRef,
    area: Uuid,
) -> Result<(), String> {
    let actor = actor(host, player).ok_or("无法读取玩家身份")?;
    let (dimension, pos) = location(host, player)?;
    let (session, seconds) = {
        let service = modification.service();
        let settings = &service.ready()?.settings;
        let record = service
            .index
            .areas
            .get(&area)
            .ok_or("领地已发生变化，请重新打开菜单")?;
        if !actor.manages(settings, record) {
            return Err("只有领地主人或管理员可以管理".into());
        }
        let seconds = settings.member_input_timeout.clamp(5, 300);
        let session = MemberInput {
            area,
            standing_in: service.at(&dimension, pos).map(|area| area.id),
            dimension,
            expires: service.ticks.saturating_add(seconds * 20),
            input: None,
        };
        (session, seconds)
    };
    host.close_menu(player.uuid());
    modification
        .service()
        .member_inputs
        .insert(actor.uuid, session);
    tell(
        host,
        player,
        format!("请直接在聊天中输入玩家名称（仅自己可见），输入 取消 可退出；{seconds} 秒内有效"),
    );
    Ok(())
}

pub fn capture_input(modification: &Mod, player: u128, raw: &str) -> bool {
    let mut service = modification.service();
    if let Some(session) = service.member_inputs.get_mut(&Uuid::from_u128(player)) {
        session.input.get_or_insert_with(|| raw.trim().to_owned());
        return true;
    }
    false
}

pub fn poll_inputs(modification: &Mod, host: &mut dyn ServerHost) {
    let sessions = {
        let mut service = modification.service();
        let tick = service.ticks;
        service
            .member_inputs
            .extract_if(|_, session| session.input.is_some() || tick >= session.expires)
            .collect::<Vec<_>>()
    };
    for (id, session) in sessions {
        if let Some(player) = host.player(id.as_u128()) {
            let reply = match session.input.as_ref() {
                None => "添加成员已超时，请重新打开菜单".into(),
                Some(input) if input.eq_ignore_ascii_case("cancel") || input == "取消" => {
                    "已取消添加成员".into()
                }
                Some(input) => accept_member_input(modification, host, player, &session, input)
                    .map_or_else(|err| err, |()| format!("已添加领地成员：{input}")),
            };
            tell(host, player, reply);
        }
    }
}

fn accept_member_input(
    modification: &Mod,
    host: &dyn ServerHost,
    player: PlayerRef,
    session: &MemberInput,
    input: &str,
) -> Result<(), String> {
    let actor = actor(host, player).ok_or("无法读取玩家身份")?;
    let (dimension, pos) = location(host, player)?;
    let mut service = modification.service();
    if dimension != session.dimension
        || service.at(&dimension, pos).map(|area| area.id) != session.standing_in
    {
        return Err("位置或领地已发生变化，请重新打开菜单".into());
    }
    service.add_member(session.area, &actor, input)
}

fn location(host: &dyn ServerHost, player: PlayerRef) -> Result<(String, [i32; 3]), String> {
    let dimension = player
        .dimension(host)
        .ok_or("无法读取当前维度")?
        .to_string();
    let pos = player
        .entity()
        .block_position(host)
        .ok_or("无法读取玩家坐标")?;
    Ok((dimension, pos))
}

fn clamp_page(page: usize, count: usize, capacity: usize) -> usize {
    page.min(count.saturating_sub(1) / capacity)
}

fn icon(host: &dyn ServerHost, material: &str, name: &str, lore: &[&str]) -> Option<ItemStack> {
    let id = Identifier::from_parts("minecraft", material).ok()?;
    let mut stack = ItemStack::new(host.item_key(&id)?, 1)?;
    stack
        .set_components(
            [
                ("minecraft:custom_name".into(), cores::text(name)),
                (
                    "minecraft:lore".into(),
                    Tag::List(lore.iter().map(|line| cores::text(line)).collect()),
                ),
            ]
            .into(),
        )
        .ok()?;
    Some(stack)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Area, Volume};
    use mod_api::{prelude::ItemsHost, testing::FakeHost};

    #[test]
    fn expanding_charges_the_tagged_kit_once_and_rolls_back_payment_on_failure() {
        let directory = tempfile::tempdir().unwrap();
        let modification = Mod::default();
        let owner = Actor { uuid: Uuid::from_u128(1), name: "Owner".into(), operator: false };
        let area = Uuid::from_u128(11);
        let neighbor = Uuid::from_u128(12);
        let make_area = |id, pos| Area::new(id, "world".into(), Some(owner.uuid), owner.name.clone(),
            vec![Volume::sized(pos, [4; 3], "starter".into()).unwrap()]);
        {
            let mut service = modification.service();
            service.start(directory.path().into());
            service.index.add(make_area(area, [0, 64, 0])).unwrap();
            service.index.add(make_area(neighbor, [4, 64, 0])).unwrap();
        }

        let mut host = FakeHost::new();
        let paper = Identifier::parse("minecraft:paper").unwrap();
        host.registries.resolve_items([paper.clone()]).unwrap();
        let mut kit = ItemStack::new(host.item_key(&paper).unwrap(), 2).unwrap();
        kit.set_components([("minecraft:custom_data".into(), Tag::compound([(
            "pigeonmoe:life_skills_item", Tag::String("territory_upgrade_kit".into()),
        )]))].into()).unwrap();
        let mut imitation = ItemStack::new(host.item_key(&paper).unwrap(), 2).unwrap();
        imitation.set_components([("minecraft:custom_name".into(), cores::text("领地升级套装"))].into()).unwrap();
        let slots = &mut host.join(owner.uuid.as_u128(), 1).slots;
        slots.insert(0, imitation);
        slots.insert(1, kit);
        let selection = Expansion { area, core: [0, 64, 0], direction: ExpansionDirection::East };
        let count = |host: &FakeHost, slot| host.players.get(owner.uuid.as_u128()).unwrap().slots[&slot].count();

        let result = apply_expansion(&modification, &mut host, &owner, ("minecraft:overworld", -64..320), selection);
        assert!(result.unwrap_err().contains("重叠"));
        assert_eq!(count(&host, 1), 2);
        modification.service().index.remove(neighbor).unwrap();
        let result = apply_expansion(&modification, &mut host, &owner, ("minecraft:overworld", -64..320), selection);
        assert_eq!(result.unwrap(), "领地已扩建至 8×4×4");
        assert_eq!(count(&host, 0), 2);
        assert_eq!(count(&host, 1), 1);
        let store = crate::store::Store::new(directory.path().into());
        assert_eq!(store.load().unwrap().areas[&area].volumes[0].sides(), [8, 4, 4]);

        let save = directory.path().join("areas.yml");
        std::fs::rename(&save, directory.path().join("saved-areas.yml")).unwrap();
        std::fs::create_dir(&save).unwrap();
        assert!(apply_expansion(&modification, &mut host, &owner, ("minecraft:overworld", -64..320), selection).is_err());
        assert_eq!(count(&host, 1), 1);
        assert_eq!(modification.service().index.areas[&area].volumes[0].sides(), [8, 4, 4]);
    }
}
