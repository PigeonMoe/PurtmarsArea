use crate::{
    Mod, actor, block_position, cores,
    model::Flag,
    service::{Service, manager_area},
};
use mod_api::{
    Command, CommandArgumentValue, CommandInvocation, CommandSource, RunModCommandEvent, ServerHost,
};

const ROOTS: [&str; 3] = ["purtmarsarea", "pa", "area"];
const ADMIN: &[&str] = &[
    "item",
    "save",
    "load",
    "reload",
    "import",
    "mirror",
    "status",
    "explosions",
];

pub fn register(commands: &mut Vec<Command>) {
    for root in ROOTS {
        for usage in [
            "",
            " help",
            " info",
            " list",
            " menu",
            " add <member:word>",
            " remove <member:word>",
            " set <flag:word> <allowed:boolean> [member:word]",
            " name <name:text>",
            " welcome [message:text]",
            " farewell [message:text]",
        ] {
            commands.push(Command::new(&format!("{root}{usage}")));
        }
        for usage in [
            "save",
            "reload",
            "load",
            "import",
            "mirror",
            "explosions",
            "status",
            "item <core:word> [target:player] [count:integer]",
        ] {
            commands.push(Command::new(&format!("{root} {usage}")));
        }
    }
}

pub fn belongs_to_us(literals: &[String]) -> bool {
    literals
        .first()
        .is_some_and(|name| ROOTS.contains(&name.as_str()))
}

pub fn run(
    modification: &Mod,
    host: &mut dyn ServerHost,
    event: &RunModCommandEvent<'_>,
) -> Result<Vec<String>, String> {
    let command = event.command;
    let subcommand = command.literals.get(1).map(String::as_str).unwrap_or("");
    let mut service = modification.service();
    let admin = match *event.source {
        CommandSource::Player { player, .. } => actor(host, player).is_some_and(|actor| {
            service
                .config
                .as_ref()
                .map_or(actor.operator, |config| actor.admin(&config.settings))
        }),
        _ => event.source.permission().level() > 0,
    };
    if ADMIN.contains(&subcommand) && !admin {
        return Err("需要管理员权限".into());
    }
    let reply = match subcommand {
        "" | "help" => {
            let mut lines = vec![
                "领地管理 /pa info | menu | list | add 玩家 | remove 玩家".into(),
                "/pa set 权限 true/false [成员] | name 名称".into(),
            ];
            if admin {
                lines.push("管理：/pa item 核心编号 [在线玩家] [数量] | save | reload | import | mirror | status | explosions".into());
            }
            return Ok(lines);
        }
        "status" => format!(
            "Horizon | 领地 {} | {}",
            service.index.areas.len(),
            service
                .error
                .as_deref()
                .unwrap_or(if service.store.is_some() {
                    "正常"
                } else {
                    "未加载"
                })
        ),
        "mirror" => {
            return Ok(if service.denials.is_empty() {
                vec!["本次启动尚无拦截记录".into()]
            } else {
                service
                    .denials
                    .iter()
                    .map(|(flag, count)| format!("{} {count}", flag.key()))
                    .collect()
            });
        }
        "explosions" => {
            return Ok(if service.explosions.is_empty() {
                vec!["本次启动尚无爆炸记录".into()]
            } else {
                service.explosions.iter().cloned().collect()
            });
        }
        "save" => {
            service.save()?;
            "已保存领地".into()
        }
        "load" | "reload" => {
            service.reload()?;
            format!("已加载 {} 个领地", service.index.areas.len())
        }
        "import" => {
            let online: Vec<_> = host
                .players()
                .server_players()
                .into_iter()
                .filter_map(|player| actor(host, player).map(|actor| (actor.uuid, actor.name)))
                .collect();
            format!("已导入 {} 个旧版领地", service.import_legacy(&online)?)
        }
        "item" => {
            let id = string(command, "core").ok_or("缺少核心编号")?;
            let target = match command.argument("target") {
                Some(CommandArgumentValue::Player(player)) => *player,
                _ => match *event.source {
                    CommandSource::Player { player, .. } => player,
                    _ => return Err("请指定一名在线玩家".into()),
                },
            };
            let count = match command.argument("count") {
                Some(CommandArgumentValue::Integer(count)) if (1..=64).contains(count) => {
                    *count as u32
                }
                Some(CommandArgumentValue::Integer(_)) => return Err("数量必须在 1–64 之间".into()),
                _ => 1,
            };
            let core = service.ready()?.cores.get(id).ok_or("未知核心编号")?;
            let stack = cores::build(host, &core.item, id, count, false)?;
            let name = target.name(host).ok_or("玩家不在线")?;
            drop(service);
            target
                .give_stack(host, stack)
                .map_err(|error| format!("核心发放失败：{error:?}"))?;
            format!("已向 {name} 发放 {count} 个核心")
        }
        _ => {
            let CommandSource::Player { player, .. } = *event.source else {
                return Err("此命令需要玩家执行".into());
            };
            let actor = actor(host, player).ok_or("无法读取玩家身份")?;
            let dimension = player.dimension(host).ok_or("无法读取当前维度")?;
            let pos = player
                .entity()
                .position(host)
                .map(block_position)
                .ok_or("无法读取玩家坐标")?;
            let config = service.ready()?;
            let area = service
                .at(&dimension.to_string(), pos)
                .ok_or("当前位置没有领地")?;
            let area_id = area.id;
            if !actor.manages(&config.settings, area) && subcommand != "info" {
                return Err("只有领地主人或管理员可以管理".into());
            }
            match subcommand {
                "info" => {
                    let v = &area.volumes[0];
                    return Ok(vec![
                        format!("{} | 主人 {} | ID {}", area.name, area.owner_name, area.id),
                        format!(
                            "核心 {} {},{},{} | 大小 {}×{}×{}",
                            area.world,
                            v.core_pos[0],
                            v.core_pos[1],
                            v.core_pos[2],
                            v.max[0] - v.min[0] + 1,
                            v.max[1] - v.min[1] + 1,
                            v.max[2] - v.min[2] + 1
                        ),
                        format!(
                            "主核心边界 X {}..{} | Y {}..{} | Z {}..{}",
                            v.min[0], v.max[0], v.min[1], v.max[1], v.min[2], v.max[2]
                        ),
                        format!(
                            "成员：{} | 扩展 {}",
                            area.members
                                .values()
                                .map(|member| member.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", "),
                            area.volumes.len() - 1
                        ),
                    ]);
                }
                "add" => {
                    let name = string(command, "member").ok_or("缺少玩家名称")?;
                    if let Some(member) = host.players().player_by_name(name)
                        && let Some(member_name) = member.name(host)
                    {
                        service.remember(uuid::Uuid::from_u128(member.uuid()), &member_name)?;
                    }
                    service.add_member(area_id, &actor, name)?;
                    format!("已添加成员 {name}")
                }
                "remove" => {
                    let name = string(command, "member").ok_or("缺少玩家名称")?;
                    service.change(|index, config| {
                        let area = manager_area(index, area_id, &actor, &config.settings)?;
                        let key = Service::member_key(area, name)?;
                        area.members.remove(&key);
                        Ok(())
                    })?;
                    format!("已移除成员 {name}")
                }
                "set" => {
                    let flag = Flag::parse(string(command, "flag").ok_or("缺少权限")?)?;
                    let Some(CommandArgumentValue::Boolean(value)) = command.argument("allowed")
                    else {
                        return Err("缺少权限值".into());
                    };
                    let member = string(command, "member");
                    service.change(|index, config| {
                        if config.settings.ignore_flags.contains(&flag) {
                            return Err("管理员已锁定该权限".into());
                        }
                        let area = manager_area(index, area_id, &actor, &config.settings)?;
                        if let Some(name) = member {
                            if !flag.personal() {
                                return Err("环境权限不能单独设置给成员".into());
                            }
                            let key = Service::member_key(area, name)?;
                            area.members
                                .get_mut(&key)
                                .ok_or("成员已发生变化")?
                                .flags
                                .insert(flag, *value);
                        } else {
                            area.flags.insert(flag, *value);
                        }
                        Ok(())
                    })?;
                    format!("已设置 {} 为 {value}", flag.title())
                }
                "name" => {
                    let name = string(command, "name").ok_or("缺少领地名称")?.trim();
                    if name.is_empty() || name.encode_utf16().count() > 32 {
                        return Err("领地名称须包含 1–32 个字符".into());
                    }
                    service.change(|index, config| {
                        manager_area(index, area_id, &actor, &config.settings)?.name = name.into();
                        Ok(())
                    })?;
                    format!("领地名称已更新为 {name}")
                }
                "welcome" | "farewell" => "领地使用固定格式的占领提示".into(),
                _ => return Err("未知领地命令".into()),
            }
        }
    };
    Ok(vec![reply])
}

pub fn string<'a>(command: &'a CommandInvocation, key: &str) -> Option<&'a str> {
    match command.argument(key) {
        Some(CommandArgumentValue::String(value)) => Some(value),
        _ => None,
    }
}
