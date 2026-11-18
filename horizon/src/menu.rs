use crate::{Mod, actor, cores, model::Flag, service::manager_area, tell};
use mod_api::{
    CommandSource, Identifier, ItemStack, MenuClickedEvent, MenuReplace, PlayerRef, ServerHost,
    Tag, Text,
};
use uuid::Uuid;

const FLAGS_PER_PAGE: usize = 22;
const FLAG_SLOTS: [usize; FLAGS_PER_PAGE] = [
    10, 11, 12, 13, 14, 15, 16, 19, 20, 21, 22, 23, 24, 25, 28, 29, 30, 31, 32, 33, 34, 40,
];

#[derive(Clone, Copy)]
pub enum Page {
    Home,
    Flags(usize),
    Members(usize),
}

#[derive(Clone, Copy)]
pub struct Menu {
    pub window: i32,
    pub area: Uuid,
    pub page: Page,
}

pub fn at_player(
    modification: &Mod,
    host: &mut dyn ServerHost,
    source: &CommandSource,
) -> Result<(), String> {
    let CommandSource::Player { player, .. } = *source else {
        return Err("此命令需要玩家执行".into());
    };
    let dim = player
        .dimension(host)
        .ok_or("无法读取当前维度")?
        .to_string();
    let pos = player
        .entity()
        .block_position(host)
        .ok_or("无法读取玩家坐标")?;
    let area = modification
        .service()
        .at(&dim, pos)
        .map(|area| area.id)
        .ok_or("当前位置没有领地")?;
    open(modification, host, player, area, Page::Home)
}

pub fn open(
    modification: &Mod,
    host: &mut dyn ServerHost,
    player: PlayerRef,
    id: Uuid,
    page: Page,
) -> Result<(), String> {
    let actor = actor(host, player).ok_or("无法读取玩家身份")?;
    let (rows, title, items) = {
        let service = modification.service();
        let config = service.ready()?;
        let area = service
            .index
            .areas
            .get(&id)
            .ok_or("领地已发生变化，请重新打开菜单")?;
        if !actor.manages(area) {
            return Err("只有领地主人或管理员可以管理".into());
        }
        match page {
            Page::Home => {
                let mut items = vec![None; 9];
                items[4] = icon(
                    host,
                    "book",
                    &area.name,
                    &["左键：领地设置", "右键：成员管理"],
                );
                (1, area.name.clone(), items)
            }
            Page::Flags(page) => {
                let mut items = vec![None; 54];
                let flags: Vec<_> = Flag::ALL
                    .iter()
                    .copied()
                    .filter(|flag| !config.settings.ignore_flags.contains(flag))
                    .collect();
                let start = page.saturating_mul(FLAGS_PER_PAGE);
                for (&slot, flag) in FLAG_SLOTS.iter().zip(flags.iter().skip(start)) {
                    let allowed = area.enabled(*flag, config.settings.default_for(*flag));
                    let (material, value) = if allowed {
                        ("green_terracotta", "允许")
                    } else {
                        ("red_terracotta", "禁止")
                    };
                    items[slot] = icon(
                        host,
                        material,
                        flag.key(),
                        &[value, "左键切换，右键恢复默认"],
                    );
                }
                controls(host, &mut items, page, flags.len(), FLAGS_PER_PAGE);
                (6, "领地设置".into(), items)
            }
            Page::Members(page) => {
                let mut items = vec![None; 54];
                let slots = (1..5).flat_map(|row| (1..8).map(move |col| row * 9 + col));
                for (slot, member) in slots.zip(area.members.values().skip(page.saturating_mul(28)))
                {
                    items[slot] = icon(
                        host,
                        "player_head",
                        &member.name,
                        &["使用 /pa remove 玩家名 移除", "使用 /pa set 设置成员权限"],
                    );
                }
                items[4] = icon(host, "oak_sign", "添加成员", &["使用 /pa add 玩家名"]);
                controls(host, &mut items, page, area.members.len(), 28);
                (6, "领地成员".into(), items)
            }
        }
    };
    let window = host
        .open_menu(
            player.uuid(),
            rows,
            Text::literal(title),
            items,
            MenuReplace::InPlace,
        )
        .map_err(|e| format!("无法打开菜单：{e:?}"))?
        .ok_or("菜单打开被取消")?;
    modification.service().menus.insert(
        actor.uuid,
        Menu {
            window,
            area: id,
            page,
        },
    );
    Ok(())
}

pub fn click(modification: &Mod, host: &mut dyn ServerHost, evt: &MenuClickedEvent) {
    let uuid = Uuid::from_u128(evt.uuid);
    let current = modification.service().menus.get(&uuid).copied();
    let Some(menu) = current.filter(|menu| menu.window == evt.window) else {
        return;
    };
    let Some(player) = host.player(evt.uuid) else {
        return;
    };
    let Some(actor) = actor(host, player) else {
        return;
    };
    if evt.mode != 0 || !(0..=1).contains(&evt.button) {
        return;
    }
    let next = match (menu.page, evt.slot) {
        (Page::Home, 4) if evt.button == 0 => Some(Page::Flags(0)),
        (Page::Home, 4) => Some(Page::Members(0)),
        (Page::Home, _) => None,
        (_, 49) => Some(Page::Home),
        (Page::Flags(page), 45) if page > 0 => Some(Page::Flags(page - 1)),
        (Page::Members(page), 45) if page > 0 => Some(Page::Members(page - 1)),
        (Page::Flags(page), 53) => {
            let service = modification.service();
            let count = service.config.as_ref().map(|config| {
                Flag::ALL
                    .iter()
                    .filter(|f| !config.settings.ignore_flags.contains(f))
                    .count()
            });
            count
                .filter(|count| (page + 1) * FLAGS_PER_PAGE < *count)
                .map(|_| Page::Flags(page + 1))
        }
        (Page::Members(page), 53) => {
            let count = modification
                .service()
                .index
                .areas
                .get(&menu.area)
                .map(|area| area.members.len());
            count
                .filter(|count| (page + 1) * 28 < *count)
                .map(|_| Page::Members(page + 1))
        }
        (Page::Members(_), 4) => {
            tell(host, player, "使用 /pa add 玩家名");
            None
        }
        (Page::Flags(page), slot) if slot >= 0 => {
            let Some(offset) = FLAG_SLOTS.iter().position(|&s| s == slot as usize) else {
                return;
            };
            let result = modification.service().change(|index, config| {
                let flag = Flag::ALL
                    .iter()
                    .copied()
                    .filter(|flag| !config.settings.ignore_flags.contains(flag))
                    .nth(page * FLAGS_PER_PAGE + offset)
                    .ok_or("权限菜单已发生变化")?;
                let default = config.settings.default_for(flag);
                let area = manager_area(index, menu.area, &actor)?;
                if evt.button == 1 {
                    area.flags.remove(&flag);
                } else {
                    area.flags.insert(flag, !area.enabled(flag, default));
                }
                Ok(())
            });
            if let Err(error) = result {
                tell(host, player, error);
                return;
            }
            Some(menu.page)
        }
        _ => None,
    };
    if let Some(page) = next
        && let Err(error) = open(modification, host, player, menu.area, page)
    {
        tell(host, player, error);
    }
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

fn controls(
    host: &dyn ServerHost,
    items: &mut [Option<ItemStack>],
    page: usize,
    count: usize,
    size: usize,
) {
    items[49] = icon(host, "redstone_block", "返回", &[]);
    if page > 0 {
        items[45] = icon(host, "arrow", "上一页", &[]);
    }
    if (page + 1) * size < count {
        items[53] = icon(host, "arrow", "下一页", &[]);
    }
}
