# PurtmarsArea for Horizon

Horizon 26.3 原生服务端插件预览。Rust 插件实现位于本目录，Paper 26.2 维护在主分支。

## 在 PigeonMoe Horizon 上构建

Horizon.4 的 `purtmarsarea.hmod` 是原生源码包，对应 PigeonMoe Horizon 引擎提交 `d2679a263841152d461e921fba39007e9ba4d3a1`。使用该源码构建本地开发 SDK。生成的服务端运行时与执行构建的操作系统和 CPU 架构匹配。以下命令以 Linux 或 macOS 为例。

先准备可运行的 `horizon-launcher`、Rustup、Python 3.11 或更新版本、JDK 25、兼容的 Minecraft Java Edition 26.3 客户端安装和对应的 26.3 服务端 JAR。使用绝对路径设置 `MINECRAFT_DIR` 及 `SERVER_JAR`。

```sh
cd /absolute/path/to/pigeon-horizon
git switch --detach d2679a263841152d461e921fba39007e9ba4d3a1
export RUSTUP_TOOLCHAIN="$(cat tools/ci/rust-version)"  # 1.99.1
rustup toolchain install "$RUSTUP_TOOLCHAIN"
rustup component add rust-src --toolchain "$RUSTUP_TOOLCHAIN"
rustup target add wasm32-unknown-unknown --toolchain "$RUSTUP_TOOLCHAIN"

cargo build --release --locked -p horizon-game --bin horizon-client --no-default-features
cargo build --release --locked -p horizon-client-lib --bin horizon-ctl
cargo build --release --locked -p horizon-prepare -p horizon-server
cargo run --release --locked -p cargo-horizon-mod -- wasm-std "$PWD/target/release/horizon-wasm-std.wasm"
```

从已拥有的游戏安装导入资源。编译导出器后，将命令输出目录中的 `server/`、`client/` 和 `manifest.json` 复制到当前源码根目录的 `exporters/`。

```sh
target/release/horizon-prepare --prepare --root "$PWD" --minecraft-dir "$MINECRAFT_DIR" --server-jar "$SERVER_JAR"
target/release/horizon-prepare compile-exporters --root "$PWD"
```

安装开发版并把命令返回的完整 `developer-…` 标识赋给 `DEVELOPER_TAG`。将 SDK 输出路径设为新的绝对目录。

```sh
horizon-launcher developer-install "$PWD" --program "$PWD/target/release/horizon-client" --trust-unsigned
horizon-launcher prepare --version "$DEVELOPER_TAG" --minecraft "$MINECRAFT_DIR"
python3 tools/prebuilt_game.py build "$SDK_DIR" --server
```

将 `purtmarsarea.hmod` 解包到新的目录 `MOD_DIR`，确保该目录直接包含 `horizon-mod.toml` 和 `Cargo.toml`。选择尚不存在的绝对路径 `EXPORT_DIR`。启动器使用与开发版相同的提交、主机架构和 Rust 版本校验本地 SDK，然后导出已链接原生插件的专用引擎。

```sh
mkdir "$MOD_DIR"
unzip /path/to/purtmarsarea.hmod -d "$MOD_DIR"
HORIZON_LAUNCHER_DEVELOPER_SDK="$SDK_DIR" horizon-launcher native-build "$MOD_DIR" \
  --version "$DEVELOPER_TAG" --server-output "$EXPORT_DIR" --trust-native-code
target/release/horizon-server --dir "$SERVER_DIR" config set "engine.path=$EXPORT_DIR/horizon-engine"
```

如果服务目录已经显式设置 `engine.resources`，请确认它仍指向兼容的 26.3 资源。新导出目录的资源位于 `$EXPORT_DIR/.local/horizon-client/26.3`。

随后按服务端维护流程重启进程。通过受访问密钥保护的 `GET /api/v1/mods` 核对加载的 `purtmarsarea` ID。开发版的提交要求严格匹配。Horizon.2 源码包对应服务端预览版 `pigeon-horizon-v0.1.0-preview.1`，Horizon.1 源码包对应原上游提交。

在工作区对照同一提交的 API 源码执行测试：

```sh
mkdir -p .horizon/sdk
ln -s /absolute/path/to/pigeon-horizon .horizon/sdk/api
cargo test --locked
```

构建流程见匹配引擎仓库中的 `docs/modding/src/how-to/native-mods/07-local-developer-builds.md` 和 `docs/modding/src/how-to/native-mods/12-standalone-server-export.md`。

## 配置与存档

启动时从服务目录的 `plugins/PurtmarsArea` 读取 `config.yml` 和 `areas.yml`，也可以用环境变量 `PURTMARSAREA_DATA_DIR` 指定目录。缺少 `config.yml` 时从包内的默认配置生成文件。自定义核心材质需要重启引擎，以便注册放置完成回调。默认维度映射为 `world`、`world_nether`、`world_the_end`。如原存档的世界名称不同，请配置 `Settings.WorldAliases`，每项使用 Horizon 维度 ID 为键、既有存档的世界名称为值。

旧 Paper `areas.yml` 支持 schema 2/3。原始 `save/<世界名>.yml` 可放在 `legacy-save` 并执行 `/pa import`。同一存档避免两个引擎同时写入。旧玩家缓存中的名称只有同时存在相应 UUID 的玩家存档时才用于导入；其余仅含名称的记录在玩家以匹配名称进入时绑定 UUID。此后改名仍按 UUID 匹配，已知名称与 UUID 保存在 `known-players.yml`。读取失败会锁定插件已接管的保护路径，保留源文件。领地写入使用同目录原子替换，并留一份上一快照。

`Settings.PermissionNodes` 按玩家 UUID 设置 Paper 权限节点。管理和保护绕过分别默认继承 Horizon 的 OP 状态，普通玩家默认可以创建领地，按世界绕过放置限制默认关闭。以下配置授予一名玩家管理权限和下界创建资格，同时继续对该玩家执行领地保护规则。

```yaml
Settings:
  PermissionNodes:
    '2da65f9c-2d65-450c-a470-873559f17905':
      PurtmarsArea.command.admin: true
      PurtmarsArea.bypass: false
      PurtmarsArea.create: true
      PurtmarsArea.bypass.world_nether: true
```

在使用 PigeonMoe Horizon Panel 时，可将 `panel-plugins/purtmarsarea.toml` 复制到服务目录的 `panel-plugins/`。页面在运行时确认插件已加载后提供状态、拦截计数和最近爆炸记录。

## 功能范围

| 功能 | 当前实现 |
| --- | --- |
| 领地 | 4 种默认核心、三维边界、重叠检查、主人 UUID、成员独立权限、拆除与破碎核心。创建通过 `/pa item` 发放后放置 |
| 游戏界面 | 核心右键首页，领地与成员分页，成员独立设置及恢复默认，通过私密聊天输入成员名，Shift 点击移除。穿越边界和创建成功时显示领地标题，开启 `particle` 的领地显示边界 |
| 命令 | `/purtmarsarea`、`/pa`、`/area`。支持信息、列表、菜单、命名、成员管理、权限开关、发放、保存、重载、导入、管理诊断 |
| 玩家保护 | 方块放置和拆除、水桶、双箱、门、实体交互和近战、地面移动和客户端驱动的车辆移动、主动丢弃物品、物品与箭矢拾取。`/tp`、末影珍珠、紫颂果、观察者与原版门户传送检查原点和落点；喷溅及滞留药水逐目标检查有害效果 |
| 世界保护 | 火与流体、活塞、自然生物生成、海绵逐方块吸水、漏斗传输及吸入、发射器和投掷器、批量爆炸方块破坏。支持按零件位置冻结已接入的红石电平变化，以及按原始领地和落点保护下落方块。Slimefun 工位会同时检查输入发射器的权限，包括洗矿机上方的发射器 |
| Paper 差异 | 待补服务端驱动的矿车、探测铁轨等红石路径、部分投射物伤害及施肥和生长的次级方块变更。Paper 事件接口依赖 Bukkit，Horizon 使用原生 Rust SDK |

Rust 测试覆盖边界与重叠、权限与玩家改名、旧存档迁移、存储失败后的保护、爆炸预判、粒子采样和下落方块的原始领地。真实 Minecraft 26.3 客户端联测仍待完成。
