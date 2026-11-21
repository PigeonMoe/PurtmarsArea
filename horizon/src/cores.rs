use crate::{Service, message, presentation, store::Item, tell};
use mod_api::{Compound, Identifier, ItemPart, ItemStack, OnBlockPlacedByEvent, ServerHost, Tag};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

const CORE: &str = "purtmarsarea:core";
const BROKEN: &str = "purtmarsarea:broken-core";
const LIFE_SKILLS_ITEM: &str = "pigeonmoe:life_skills_item";
const UPGRADE_KIT: &str = "territory_upgrade_kit";

pub enum Marker {
    Core(String),
    Broken,
}

pub fn is_upgrade_kit(item: &ItemStack) -> bool {
    matches!(
        item.components()
            .and_then(|patch| patch.as_nbt().get(&"minecraft:custom_data".into()))
            .and_then(|data| data.get(LIFE_SKILLS_ITEM)),
        Some(Tag::String(id)) if id.0.iter().copied().eq(UPGRADE_KIT.encode_utf16())
    )
}

pub fn marker(item: &ItemStack) -> Option<Marker> {
    let Tag::Compound(data) = item
        .components()?
        .as_nbt()
        .get(&"minecraft:custom_data".into())?
    else {
        return None;
    };
    let tags = [
        Some(data),
        match data.get(&"PublicBukkitValues".into()) {
            Some(Tag::Compound(data)) => Some(data),
            _ => None,
        },
    ];
    if tags
        .iter()
        .flatten()
        .any(|data| matches!(data.get(&BROKEN.into()), Some(Tag::String(_))))
    {
        return Some(Marker::Broken);
    }
    tags.iter()
        .flatten()
        .find_map(|data| match data.get(&CORE.into()) {
            Some(Tag::String(id)) => Some(Marker::Core(id.to_string_lossy())),
            _ => None,
        })
}

pub fn item_id(item: &Item) -> String {
    let material = item.material.to_ascii_lowercase();
    if material.contains(':') {
        material
    } else {
        format!("minecraft:{material}")
    }
}

pub fn build(
    host: &dyn ServerHost,
    item: &Item,
    id: &str,
    count: u32,
    broken: bool,
) -> Result<ItemStack, String> {
    let item_key = Identifier::parse(&item_id(item))
        .ok()
        .and_then(|key| host.item_key(&key))
        .ok_or_else(|| format!("无效核心材质：{}", item.material))?;
    let mut stack = ItemStack::new(item_key, count).ok_or("无效核心物品数量")?;
    let marker = if broken { BROKEN } else { CORE };
    let data = Compound::from([(
        "PublicBukkitValues".into(),
        Tag::Compound(Compound::from([(marker.into(), Tag::String(id.into()))])),
    )]);
    let mut components = Compound::from([
        ("minecraft:custom_data".into(), Tag::Compound(data)),
        ("minecraft:custom_name".into(), text(&item.name)),
    ]);
    let mut lore: Vec<Tag> = item.lore.iter().map(|line| text(line)).collect();
    if broken
        && !item
            .lore
            .iter()
            .any(|line| line.contains("修补后可继续使用"))
    {
        lore.push(text("&7修补后可继续使用"));
    }
    if !lore.is_empty() {
        components.insert("minecraft:lore".into(), Tag::List(lore));
    }
    if item.shiny {
        components.insert("minecraft:enchantment_glint_override".into(), Tag::Byte(1));
    }
    stack
        .set_components(components)
        .map_err(|e| format!("{e:?}"))?;
    Ok(stack)
}

pub fn plain_color(value: &str) -> String {
    let mut chars = value.chars().peekable();
    let mut plain = String::new();
    while let Some(char) = chars.next() {
        if matches!(char, '&' | '§')
            && chars
                .peek()
                .is_some_and(|next| "0123456789abcdefklmnor".contains(next.to_ascii_lowercase()))
        {
            chars.next();
        } else {
            plain.push(char);
        }
    }
    plain
}

const COLORS: &[&str] = &[
    "black",
    "dark_blue",
    "dark_green",
    "dark_aqua",
    "dark_red",
    "dark_purple",
    "gold",
    "gray",
    "dark_gray",
    "blue",
    "green",
    "aqua",
    "red",
    "light_purple",
    "yellow",
    "white",
];

pub fn text(value: &str) -> Tag {
    let mut children = Vec::new();
    let mut buffer = String::new();
    let mut style = TextFormat::default();
    let mut chars = value.chars().peekable();
    while let Some(char) = chars.next() {
        let Some(code) = chars
            .peek()
            .copied()
            .filter(|next| "0123456789abcdefklmnor".contains(next.to_ascii_lowercase()))
            .filter(|_| matches!(char, '&' | '§'))
        else {
            buffer.push(char);
            continue;
        };
        chars.next();
        if !buffer.is_empty() {
            children.push(style.component(&std::mem::take(&mut buffer)));
        }
        if let Some(color) = code.to_digit(16) {
            style = TextFormat {
                color: COLORS[color as usize],
                ..TextFormat::default()
            };
        } else if code.eq_ignore_ascii_case(&'r') {
            style = TextFormat::default();
        } else {
            style.decorations.insert(code.to_ascii_lowercase());
        }
    }
    if !buffer.is_empty() {
        children.push(style.component(&buffer));
    }
    if children.len() == 1 {
        return children.pop().unwrap();
    }
    Tag::Compound(Compound::from([
        ("text".into(), Tag::String("".into())),
        ("italic".into(), Tag::Byte(0)),
        ("extra".into(), Tag::List(children)),
    ]))
}

struct TextFormat {
    color: &'static str,
    decorations: std::collections::BTreeSet<char>,
}

impl Default for TextFormat {
    fn default() -> Self {
        Self {
            color: "white",
            decorations: Default::default(),
        }
    }
}

impl TextFormat {
    fn component(&self, value: &str) -> Tag {
        let mut node = Compound::from([
            ("text".into(), Tag::String(value.into())),
            ("color".into(), Tag::String(self.color.into())),
            (
                "italic".into(),
                Tag::Byte(i8::from(self.decorations.contains(&'o'))),
            ),
        ]);
        for (code, key) in [
            ('k', "obfuscated"),
            ('l', "bold"),
            ('m', "strikethrough"),
            ('n', "underlined"),
        ] {
            if self.decorations.contains(&code) {
                node.insert(key.into(), Tag::Byte(1));
            }
        }
        Tag::Compound(node)
    }
}

pub struct CorePlacement(pub Arc<Mutex<Service>>);

#[mod_api::horizon_part]
impl ItemPart for CorePlacement {
    fn on_block_placed_by(&self, host: &mut dyn ServerHost, event: &mut OnBlockPlacedByEvent<'_>) {
        let Some(player) = event.placer else { return };
        let Some(Marker::Core(core_id)) = marker(event.stack) else {
            return;
        };
        let key = Uuid::from_u128(player.uuid());
        let result = {
            let mut service = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
            let Some(area) = service.pending_placements.remove(&key) else {
                return;
            };
            if area.volumes[0].core_pos != event.pos || area.volumes[0].core != core_id {
                return;
            }
            let id = area.id;
            service
                .change(|index, _| index.add(area))
                .map(|_| service.occupation(key, id))
        };
        match result {
            Ok(owner) => {
                tell(host, player, "领地已创建，右键核心进行管理");
                if let Some(owner) = owner {
                    presentation::occupation(host, player, &owner);
                }
            }
            Err(error) => {
                // The game has placed the block but has not consumed the stack yet.
                // Returning the item is only safe after the changed block was removed.
                if let Some(level) = player.level(host)
                    && level.remove_block(host, event.pos, false)
                    && let Some(mut refund) = ItemStack::new(event.stack.item(), 1)
                    && refund
                        .set_components(
                            event
                                .stack
                                .components()
                                .map(|parts| parts.as_nbt().clone())
                                .unwrap_or_default(),
                        )
                        .is_ok()
                {
                    let [x, y, z] = event.pos.map(|coord| f64::from(coord) + 0.5);
                    if !player.is_creative(host).unwrap_or(false) {
                        let _ = level.spawn_item(host, x, y, z, refund, 10);
                    }
                }
                let _ = player.tell(host, message(format!("无法保存领地：{error}")));
            }
        }
    }
}
