use crate::{Service, message, store::Item, tell};
use mod_api::{Compound, Identifier, ItemPart, ItemStack, OnBlockPlacedByEvent, ServerHost, Tag};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

const CORE: &str = "purtmarsarea:core";
const BROKEN: &str = "purtmarsarea:broken-core";

pub enum Marker {
    Core(String),
    Broken,
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

pub fn text(value: &str) -> Tag {
    let (style, label) = match value.get(..2) {
        Some("&e") => ("yellow", &value[2..]),
        Some("&7") => ("gray", &value[2..]),
        Some("&f") => ("white", &value[2..]),
        Some("&6") => ("gold", &value[2..]),
        Some("&a") => ("green", &value[2..]),
        Some("&c") => ("red", &value[2..]),
        _ => ("white", value),
    };
    Tag::Compound(Compound::from([
        ("text".into(), Tag::String(label.into())),
        ("color".into(), Tag::String(style.into())),
        ("italic".into(), Tag::Byte(0)),
    ]))
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
            service.change(|index, _| index.add(area)).map(|_| id)
        };
        match result {
            Ok(_) => tell(host, player, "领地已创建，右键核心进行管理"),
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
