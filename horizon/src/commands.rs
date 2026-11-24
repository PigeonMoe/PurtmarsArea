use crate::{
    Mod, actor, block_position, cores,
    model::Flag,
    service::{Actor, Service, manager_area},
};
use mod_api::{
    Command, CommandArgumentError, CommandArgumentKind, CommandArgumentType, CommandArgumentValue,
    CommandInvocation, CommandSender, CommandSource, CommandSuggestion,
    PlayerCommandLiteralsSentEvent, RunModCommandEvent, ServerHost, Suggestion, TabCompleteEvent,
};

const ROOTS: [&str; 3] = ["purtmarsarea", "pa", "area"];
const COMPLETION_WORD: &str = "purtmarsarea:completion_word";
const HIDDEN_FROM_PLAYERS: &[&str] = &["welcome", "farewell"];
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

pub(super) struct CompletionWord;

impl CommandArgumentType for CompletionWord {
    fn kind(&self) -> CommandArgumentKind {
        CommandArgumentKind::Word
    }

    fn parse(&self, input: &str) -> Result<String, CommandArgumentError> {
        Ok(input.to_owned())
    }

    fn suggest(&self, _: Option<&CommandSender>, _: &str) -> Vec<CommandSuggestion> {
        // The server's tab_complete hook can read the current player and claim.
        Vec::new()
    }
}

pub fn register(commands: &mut Vec<Command>) {
    commands.push(Command::new("purtmarsarea life_menu <target:player>"));
    for root in ROOTS {
        for usage in [
            "",
            " help",
            " info",
            " list",
            " travel",
            " menu",
            " add <member:purtmarsarea:completion_word>",
            " remove <member:purtmarsarea:completion_word>",
            " set <flag:purtmarsarea:completion_word> <allowed:purtmarsarea:completion_word> [member:purtmarsarea:completion_word]",
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
            "item <core:purtmarsarea:completion_word> [target:purtmarsarea:completion_word] [count:integer]",
        ] {
            commands.push(Command::new(&format!("{root} {usage}")));
        }
    }
}

fn player_admin(service: &Service, actor: &Actor) -> bool {
    service
        .config
        .as_ref()
        .map_or(actor.operator, |config| actor.admin(&config.settings))
}

pub fn hide_literals(modification: &Mod, event: &mut PlayerCommandLiteralsSentEvent<'_>) {
    let actor = Actor {
        uuid: uuid::Uuid::from_u128(event.player),
        name: event.name.to_owned(),
        operator: event.permission > 0,
    };
    let admin = player_admin(&modification.service(), &actor);
    for root in ROOTS {
        for hidden in HIDDEN_FROM_PLAYERS {
            event.hidden_literals.push(format!("{root} {hidden}"));
        }
        if !admin {
            for hidden in ADMIN {
                event.hidden_literals.push(format!("{root} {hidden}"));
            }
        }
    }
    event.hidden_literals.push("purtmarsarea life_menu".into());
}

fn visible_to_player(subcommand: &str, admin: bool) -> bool {
    subcommand != "life_menu"
        && !HIDDEN_FROM_PLAYERS.contains(&subcommand)
        && (admin || !ADMIN.contains(&subcommand))
}

pub fn complete(modification: &Mod, host: &dyn ServerHost, event: &mut TabCompleteEvent<'_>) {
    let [start, end] = event.range;
    let Some(before) = event.command.get(..start) else {
        return;
    };
    let Some(prefix) = event.command.get(start..end) else {
        return;
    };
    let mut words = before.split_whitespace();
    if !words.next().is_some_and(|root| ROOTS.contains(&root)) {
        return;
    }
    let Some(player) = host.player(event.player) else {
        return;
    };
    let Some(actor) = actor(host, player) else {
        return;
    };
    let service = modification.service();
    let admin = player_admin(&service, &actor);
    let Some(subcommand) = words.next().map(str::to_ascii_lowercase) else {
        event
            .suggestions
            .retain(|suggestion| visible_to_player(&suggestion.text, admin));
        return;
    };
    if !visible_to_player(&subcommand, admin) {
        event.suggestions.clear();
        return;
    }
    let Some(config) = service.config.as_ref() else {
        return;
    };
    let choices: Vec<String> = match (subcommand.as_str(), words.count()) {
        ("item", 0) => config.cores.keys().cloned().collect(),
        ("add", 0) | ("item", 1) => host
            .players()
            .server_players()
            .into_iter()
            .filter_map(|player| player.name(host))
            .collect(),
        ("set", 0) => Flag::ALL
            .iter()
            .copied()
            .filter(|flag| !config.settings.ignore_flags.contains(flag))
            .map(|flag| flag.key().into())
            .collect(),
        ("set", 1) => vec!["true".into(), "false".into()],
        ("remove", 0) | ("set", 2) => {
            let Some(dimension) = player.dimension(host) else {
                return;
            };
            let Some(pos) = player.entity().position(host).map(block_position) else {
                return;
            };
            let Some(area) = service.at(&dimension.to_string(), pos) else {
                return;
            };
            if !actor.manages(&config.settings, area) {
                return;
            }
            area.members
                .values()
                .map(|member| member.name.clone())
                .collect()
        }
        _ => return,
    };
    let prefix = prefix.to_lowercase();
    event.suggestions.clear();
    event.suggestions.extend(
        choices
            .into_iter()
            .filter(|choice| choice.to_lowercase().starts_with(&prefix))
            .map(|choice| Suggestion::new((start, end), choice)),
    );
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
        CommandSource::Player { player, .. } => {
            actor(host, player).is_some_and(|actor| player_admin(&service, &actor))
        }
        _ => event.source.permission().level() > 0,
    };
    if ADMIN.contains(&subcommand) && !admin {
        return Err("需要管理员权限".into());
    }
    let reply = match subcommand {
        "" | "help" => {
            let mut lines = vec![
                "领地管理 /pa info | menu | list | add 玩家 | remove 玩家".into(),
                "领地传送 /pa travel".into(),
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
            let count = service.index.areas.len();
            drop(service);
            host.commands().refresh_commands();
            format!("已加载 {count} 个领地")
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
            let target = match string(command, "target") {
                Some(name) => host.players().player_by_name(name).ok_or("玩家不在线")?,
                None => match *event.source {
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
            cores::give_naturally(host, target, stack)?;
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
                    let value = match string(command, "allowed")
                        .map(str::to_ascii_lowercase)
                        .as_deref()
                    {
                        Some("true") => true,
                        Some("false") => false,
                        _ => return Err("权限值只能为 true 或 false".into()),
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
                                .insert(flag, value);
                        } else {
                            area.flags.insert(flag, value);
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
        Some(CommandArgumentValue::Custom { kind, value }) if kind == COMPLETION_WORD => {
            Some(value)
        }
        _ => None,
    }
}
