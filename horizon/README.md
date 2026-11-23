# PurtmarsArea for Horizon

Horizon 26.3 原生服务端插件预览。Rust 插件实现位于本目录，Paper 26.2 维护在主分支。

## 在 PigeonMoe Horizon 上构建

Horizon.13 的 `purtmarsarea.hmod` 是原生源码包，对应 PigeonMoe Horizon 引擎提交 `c5f0f6284bdd8d4bb026f51d6d51192395794931`。使用该源码构建本地开发 SDK。生成的服务端运行时与执行构建的操作系统和 CPU 架构匹配。以下命令以 Linux 或 macOS 为例。

先准备可运行的 `horizon-launcher`、Rustup、Python 3.11 或更新版本、JDK 25、兼容的 Minecraft Java Edition 26.3 客户端安装和对应的 26.3 服务端 JAR。使用绝对路径设置 `MINECRAFT_DIR` 及 `SERVER_JAR`。

```sh
cd /absolute/path/to/pigeon-horizon
git switch --detach c5f0f6284bdd8d4bb026f51d6d51192395794931
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

## 创建与扩建

在原版工作台用 8 个箱子围住 1 个普通玻璃，合成 **初始领地核心**。放置后保护 4×4×4 格。右击核心，进入「扩建领地」即可查看六个方向的可扩建尺寸。扩建会消耗背包中的 1 个生活技能「领地升级套装」，选定的一面向外增加最多 4 格。

`Settings.MaxExpansionSideLength` 为 X/Z 共用的边长上限，默认为 64。Y 轴扩建可到达当下维度的建筑高度上下界。与现有领地重叠、超出世界边界或无法保存时，扩建失败并保留套装。旧核心和已经保存的边界按原尺寸保留。

领地主人和管理员可在主手持生活技能「扩建标尺」右击领地核心，或在获准管理的领地内右击空气，打开「扩建预览」。预览列出六个方向的可扩建尺寸、坐标和限制。点击可行方向可临时看到仅自己可见的边界粒子。预览无需消耗物品。

## 领地传送

运行 `/purtmarsarea travel`、`/pa travel` 或 `/area travel`，从当前维度的领地列表选择目的地。列表包含自己的领地，以及你可进入和传送的受信任领地。服务端检查出发位置的传送权限，并在目标领地主核心附近寻找安全落点。每次成功传送消耗背包中的 1 个生活技能「领地传送符」。失败时保留传送符。

## 插件间领地事件

支持 `player_position_committed` 和 `mod_event` 的 PigeonMoe 引擎会向订阅 `mod_event` 的原生服务端插件投递 `purtmarsarea:area_leave` 和 `purtmarsarea:area_enter`。从一块领地直接进入另一块时，离开事件先于进入事件。同一领地内移动、两端均在荒地和被拒绝的移动或传送均无事件。

本地开发版本中的 `[native]` 锁定了支持这两个接口的 Horizon 提交。构建和加载该版本时，SDK 与引擎均须使用清单中锁定的提交。

事件负载是 UTF-8 JSON。顶层 `schema` 当前为 `1`，`player` 包含 UUID 字符串 `uuid` 和账号名 `name`。`area` 包含领地 UUID `id`、存档世界名 `world`、主人 UUID `owner_uuid`（旧存档尚未绑定时为 `null`）、`owner_name`、`name`、`welcome`、`farewell`、权限 `flags`、成员 `members` 和保护区块列表 `volumes`。每个保护区块按 XYZ 顺序使用 `core_pos`、`min`、`max`，边界含端点，`core` 是配置中的核心类型。

## 功能范围

| 功能 | 当前实现 |
| --- | --- |
| 领地 | 工作台合成初始核心、旧核心兼容、指定方向扩建、三维边界、重叠检查、主人 UUID、成员独立权限、拆除与破碎核心 |
| 游戏界面 | 核心右键首页，领地与成员分页，成员独立设置及恢复默认，通过私密聊天输入成员名，Shift 点击移除。穿越边界和创建成功时显示领地标题，开启 `particle` 的领地显示边界 |
| 命令 | `/purtmarsarea`、`/pa`、`/area`。支持信息、列表、菜单、命名、成员管理、权限开关、发放、保存、重载、导入、管理诊断 |
| 玩家保护 | 方块放置和拆除、水桶、双箱、门、龙蛋左右键、实体交互、地面及乘骑移动、主动丢弃、物品与箭矢拾取。使用拴绳或剪刀批量操作时逐个检查生物及船只。`/tp`、末影珍珠、紫颂果、观察者与原版门户传送检查原点和落点。喷溅和滞留药水逐目标检查有害效果。实体伤害按负责玩家和受击实体检查相应权限。载具受伤另外检查自然保护或攻击者的建造权限 |
| 世界保护 | 火与流体、活塞、自然生物生成、海绵逐方块吸水、漏斗和漏斗矿车的传输与吸入、发射器和投掷器、爆炸、下落方块。压力板、绊线、木按钮和大垂滴叶的踩踏按触发实体检查，红石电平另行检查。末影人、兔子、蠹虫、船及弹射物改变方块前检查建造权限，包括紫颂花、饰纹陶罐、滴水石和叶片倾斜。树木、巨型蘑菇、骨粉造成的次级变更，以及草地、藤蔓、竹子、幽匿和滴水石等扩散逐目标检查。核心还检查邻居通知和形状更新。Slimefun 工位同时检查输入发射器，包括洗矿机和压力机的上层发射器 |
| 载具回退 | 其他插件可更改回退的坐标、朝向及目的世界。跨世界时逐个检查载具及乘客的权限，保留获准乘客的座位。载具回退被拒后卸下直接乘客，并单独检查首位被拒乘客能否返回。客户端驱动船只先检查驾驶员的移动数据包，再在服务端 tick 检查乘客 |

Rust 测试覆盖边界与重叠、权限与玩家改名、旧存档迁移、存储失败后的保护、爆炸预判、粒子采样、下落方块来源、伤害责任、生长筛选和压力机结构。引擎使用实际 26.3 方块资源验证生长与施肥的逐方块拦截、坐骑库存位置、漏斗矿车移动子步的端点和物品保留、被拒绝入境时矿车停回原位并保留座位、压力板与叶片的实体触发、船撞睡莲及弹射物命中的掉落。Horizon.12 曾与 PigeonAuth、Slimefun 在同一服务端进行 26.3 客户端联测，领地部分记录了 22 个用例，新加入的插件间领地事件需随新引擎提交联测。
