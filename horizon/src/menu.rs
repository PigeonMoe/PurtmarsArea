use crate::{Mod, actor, cores, model::Flag, service::manager_area, tell};
use mod_api::{
    CommandSource, Identifier, ItemStack, MenuClickedEvent, MenuReplace, PlayerRef, ServerHost,
    Tag, Text,
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
    Home(Uuid),
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
    let page = if list {
        Page::List(0)
    } else {
        let (dimension, pos) = location(host, player)?;
        let area = modification
            .service()
            .at(&dimension, pos)
            .map(|area| area.id)
            .ok_or("当前位置没有领地")?;
        Page::Home(area)
    };
    open(modification, host, player, page)
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
                        Action::Open(Page::Home(area.id)),
                    );
                }
                view.pages(host, page, areas.len(), 45, Page::List);
                view
            }
            Page::Home(id) => {
                area(id)?;
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
                    Page::Home(id)
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
                    Action::Open(Page::Home(id)),
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
    };
    open(modification, host, player, next)
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
