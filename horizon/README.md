# PurtmarsArea for Horizon

Horizon 26.3 原生服务端插件预览。Rust 插件实现位于本目录，Paper 26.2 维护在主分支。

## 构建与配置

发布资产 `purtmarsarea.hmod` 是 Horizon 的原生源码包。它需要为清单指定的引擎版本构建原生服务端运行时。本次校验的 API 源码为 `horizon-rust/horizon@dc215e79e27d1cf55bb041a2c1bf2c02a701cd74`。该提交目前没有可下载的签名 SDK。需要从对应引擎源码构建开发版 SDK、准备兼容的 Minecraft 26.3 资源，再用 Horizon Launcher 导出原生服务端运行时。

在工作区对照 API 源码执行测试：

```sh
mkdir -p .horizon/sdk
ln -s /absolute/path/to/matching/horizon .horizon/sdk/api
cargo test --locked
```

官方构建流程见 [开发版 SDK](https://github.com/horizon-rust/horizon/blob/dc215e79e27d1cf55bb041a2c1bf2c02a701cd74/docs/modding/src/how-to/native-mods/07-local-developer-builds.md) 和 [原生服务端导出](https://github.com/horizon-rust/horizon/blob/dc215e79e27d1cf55bb041a2c1bf2c02a701cd74/docs/modding/src/how-to/native-mods/12-standalone-server-export.md)。

启动时从服务目录的 `plugins/PurtmarsArea` 读取 `config.yml` 和 `areas.yml`，也可以用环境变量 `PURTMARSAREA_DATA_DIR` 指定目录。缺少配置时使用编译内置的 `data/purtmarsarea/config.yml`。自定义核心材质需要重启引擎，以便注册放置完成回调。默认维度映射为 `world`、`world_nether`、`world_the_end`。如原存档的世界名称不同，请配置 `Settings.WorldAliases`，每项使用 Horizon 维度 ID 为键、既有存档的世界名称为值。

旧 Paper `areas.yml` 支持 schema 2/3。原始 `save/<世界名>.yml` 可放在 `legacy-save` 并执行 `/pa import`。同一存档避免两个引擎同时写入。导入前核验旧版仅记录名称的主人和成员，含 UUID 的记录优先按 UUID 匹配。读取失败会锁定插件已接管的保护路径，保留源文件。写入使用同目录原子替换，并留一份上一快照。

在使用 PigeonMoe Horizon Panel 时，可将 `panel-plugins/purtmarsarea.toml` 复制到服务目录的 `panel-plugins/`。页面在运行时确认插件已加载后提供状态、拦截计数和最近爆炸记录。

## 功能范围

| 功能 | 当前实现 |
| --- | --- |
| 领地 | 4 种默认核心、三维边界、重叠检查、主人 UUID、成员独立权限、拆除与破碎核心。创建通过 `/pa item` 发放后放置 |
| 游戏界面 | 核心右键、首页、设置开关及分页、成员列表。进入和离开显示动作栏提示 |
| 命令 | `/purtmarsarea`、`/pa`、`/area`。支持信息、列表、菜单、命名、成员管理、权限开关、发放、保存、重载、导入、管理诊断 |
| 保护 | 玩家方块放置和拆除、桶、常见容器与门、实体交互和近战、地面移动、拾取物品、火与流体蔓延、活塞、漏斗部分路径、爆炸整批方块破坏。Slimefun 当前工作台/栅栏布局也检查下方发射器的领地权限 |
| 仍待验证或移植 | 传送两端与骑乘、箭矢拾取和防丢弃、完整红石/漏斗与自然生物生成、药水和投射物、落沙和方块形态变化、边界粒子、Paper 权限节点细分、跨重启玩家缓存、完整成员菜单交互及标题字幕 |

预览使用 Horizon OP 等级授予管理和普通保护绕过。已运行 Rust 测试及 Horizon 包校验。尚无连接真实 Minecraft 客户端的验收结果。生产存档先在独立测试环境核验保护路径。
