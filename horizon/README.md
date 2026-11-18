# PurtmarsArea for Horizon

Horizon 26.3 原生服务端插件预览。Rust 插件实现位于本目录，Paper 26.2 维护在主分支。

## 在 PigeonMoe Horizon 上构建

`v2.1.1-horizon.2` 的 `purtmarsarea.hmod` 是原生源码包，对应 PigeonMoe 服务端预览版 `pigeon-horizon-v0.1.0-preview.1`，引擎提交 `96addbc4b2e07f8241d403ac5522f0baefcc4972`。为此提交可以自行构建并使用本地、未签名的开发版 SDK。生成的服务端运行时与执行构建的操作系统和 CPU 架构匹配。以下以 Linux 或 macOS 终端为例。

先准备可运行的 `horizon-launcher`、Rustup、Python 3.11 或更新版本、JDK 25、兼容的 Minecraft Java Edition 26.3 客户端安装和对应的 26.3 服务端 JAR。使用绝对路径设置 `MINECRAFT_DIR` 及 `SERVER_JAR`。

```sh
git clone --branch pigeon-horizon-v0.1.0-preview.1 https://github.com/PigeonMoe/horizon.git pigeon-horizon
cd pigeon-horizon
test "$(git rev-parse HEAD)" = 96addbc4b2e07f8241d403ac5522f0baefcc4972
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

将发布的 `purtmarsarea.hmod` 解包到新的目录 `MOD_DIR`，确保该目录直接包含 `horizon-mod.toml` 和 `Cargo.toml`。选择尚不存在的绝对路径 `EXPORT_DIR`。启动器使用与开发版相同的提交、主机架构和 Rust 版本校验本地 SDK，然后导出已链接原生插件的专用引擎。

```sh
mkdir "$MOD_DIR"
unzip /path/to/purtmarsarea.hmod -d "$MOD_DIR"
HORIZON_LAUNCHER_DEVELOPER_SDK="$SDK_DIR" horizon-launcher native-build "$MOD_DIR" \
  --version "$DEVELOPER_TAG" --server-output "$EXPORT_DIR" --trust-native-code
target/release/horizon-server --dir "$SERVER_DIR" config set "engine.path=$EXPORT_DIR/horizon-engine"
```

如果服务目录已经显式设置 `engine.resources`，请确认它仍指向兼容的 26.3 资源。新导出目录的资源位于 `$EXPORT_DIR/.local/horizon-client/26.3`。

随后按服务端维护流程重启进程。通过受访问密钥保护的 `GET /api/v1/mods` 核对加载的 `purtmarsarea` ID。开发版的提交要求严格匹配，因此请配套使用 Horizon.2 和指定的 PigeonMoe 预览引擎。Horizon.1 源码包对应原上游提交。

在工作区对照同一提交的 API 源码执行测试：

```sh
mkdir -p .horizon/sdk
ln -s /absolute/path/to/pigeon-horizon .horizon/sdk/api
cargo test --locked
```

官方构建流程见 [开发版 SDK](https://github.com/PigeonMoe/horizon/blob/pigeon-horizon-v0.1.0-preview.1/docs/modding/src/how-to/native-mods/07-local-developer-builds.md) 和 [原生服务端导出](https://github.com/PigeonMoe/horizon/blob/pigeon-horizon-v0.1.0-preview.1/docs/modding/src/how-to/native-mods/12-standalone-server-export.md)。

## 配置与存档

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

预览使用 Horizon OP 等级授予管理和普通保护绕过。已运行 Rust 测试及 Horizon 包校验。本机尚未生成开发版引擎运行时，也没有真实 Minecraft 客户端的验收结果。生产存档先在独立测试环境核验保护路径。
