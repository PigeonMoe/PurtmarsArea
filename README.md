# PurtmarsArea 2.1

面向 **Paper 26.2 / Java 25+** 的领地核心插件。基础框架使用 **TabooLib 6.3.0-0e3a911**，Gradle 构建插件 **2.0.39**。不需要安装旧版独立 TabooLib 插件。

2026-10-08 核对官方 Maven 元数据及官方源码：`6.3.0-0e3a911` 是当前最新非测试构建，对应官方源码提交 `0e3a911fc55624075b5c9abd4368cb5b063b022b`。仓库还有更新的 `test2` 构建；本项目固定正式构建，保证可复现。

- [TabooLib 官方构件元数据](https://repo.tabooproject.org/repository/releases/io/izzel/taboolib/common/maven-metadata.xml)
- [TabooLib 源码版本](https://github.com/TabooLib/taboolib/tree/0e3a911fc55624075b5c9abd4368cb5b063b022b)
- [构建插件版本](https://plugins.gradle.org/plugin/io.izzel.taboolib/2.0.39)

## 功能

- 放置领地核心创建三维领地；右键核心进入箱子菜单。
- 默认四种核心：小型 8³、中型 16³、大型 32³、超大型 64³；`dimensions` 为实际方块边长，旧 `size` 配置仍按半径解释。
- 重叠检测使用完整空间交集，覆盖负坐标、跨区块以及未加载区块。
- 新核心的完整保护范围不能与任何已有核心重叠，包括同一玩家的核心；允许紧邻但不共享方块。旧存档的扩展继续保留，拆除旧主核心前须先拆除其扩展。
- 拆除后掉落对应的破碎核心，说明包含“修补后可继续使用”；破碎物品有独立持久标记，不能作为完整核心放置。当前不提供修补配方或经济接口。
- 主人 UUID、成员 UUID、成员独立权限、成员上限与领地数量限制。
- 9 格书本首页、54 格领地设置和成员管理菜单、领地列表、中文命令与补全。点击添加成员后直接私聊输入玩家名称，支持取消、超时及状态复查。
- 金色“私人领地”标题与灰白“已被 实际主人 占领.”副标题；成功创建、进入和离开时显示，同一玩家同一领地 2 秒内限一次。跨领地显示目标主人，离开至荒地显示刚离开的主人。
- 粒子边界支持显示距离、间隔、周期和每玩家粒子预算，全部在主线程调用世界 API。
- 世界白名单、荒地动作限制、锁定权限、管理员管理权限和单独的保护绕过权限。
- YAML 持久化、原子替换、上一版本备份；配置或数据损坏时保留文件并进入保护锁定，修复后可重载。
- 每次领地修改立即保存；保存失败回读最后一次成功的数据。后续插件取消核心放置时回滚相应领地。
- 旧 PurtmarsArea 1.2 的 `save/<world>.yml` 可显式导入；源文件保留。

保护规则保留旧版 23 项，并增加爆炸、火焰、PvP：

| 范围 | 权限 ID |
| --- | --- |
| 建筑、移动、传送 | `build`, `move`, `teleport` |
| 容器、附魔台、铁砧、门、蛋糕 | `container`, `enchant-table`, `anvil`, `door`, `cake` |
| 丢弃、拾取 | `item-drop`, `item-pick` |
| 生物伤害、生成 | `damage-animals`, `damage-monster`, `spawn-animals`, `spawn-monster` |
| 红石触发、运行 | `redstone`, `redstone-perform` |
| 液体 | `flow-lava`, `flow-water` |
| 交易、修剪、牵引、坠落 | `villager-trade`, `shear`, `leash`, `falling` |
| 边界粒子 | `particle` |
| 新增 | `explosion`, `fire`, `pvp` |

事件覆盖方块、水桶、挂饰、盔甲架、箱子双侧、实体容器、车辆、掉落物与箭、发射物伤害、有害药水、传送两端、活塞、发射器、漏斗、液体、爆炸、火焰和外来落沙。核心还独立防止活塞移动、爆炸、融化、物理更新和生长替换。

成员遵循旧插件的独立默认值，缺省项读取服务器 `Settings.Flags` 而非领地访客开关：添加成员不会自动允许建筑或开箱；应使用 `/pa set build true 玩家名` 等指令授权。环境规则不能单独设置给成员。

## 安装与首次使用

1. 备份原有插件、配置、领地存档和世界。
2. 将 `dist/PurtmarsArea.jar` 放入 Paper 26.2 的 `plugins`。同名旧插件应先移出加载目录。
3. 正常重启服务器。TabooLib 引导器在首次加载时从官方仓库下载框架模块及 Kotlin 依赖，服务器需要能访问相关构件仓库。
4. 在 `plugins/PurtmarsArea/config.yml` 设置 `Settings.EnableWorld` 和核心种类，执行 `/pa reload`。
5. 管理员使用 `/pa item 0 玩家名` 给予核心；玩家放置、右键管理。

默认编号 `0/1/2/3` 分别对应 `8/16/32/64` 格。偶数边长以核心坐标为锚点，每轴从 `-边长/2` 到 `+(边长/2-1)`，例如 8 格覆盖 `-4..+3`。范围不能超出世界高度，核心材质须为不受重力影响的实心方块。默认白色染色玻璃符合要求。

**升级已有安装不会自动覆盖 config.yml**。要启用新的四种默认核心，应按 [MIGRATION.md](MIGRATION.md) 合并配置；已有 `size: 8` 仍代表半径 8，已有存档的边界不变。

默认 `teleport` 在 `Settings.IgnoreFlags` 中锁定并从菜单隐藏；解锁后会重新显示。默认设置页前 22 项保留旧规则，第 2 页提供爆炸、火焰与 PvP。启用/禁用分别使用绿色/红色陶瓦，49 格红石块返回。成员每页 28 个头颅，49 格告示牌添加。

## 命令

主命令 `/purtmarsarea`，别名 `/pa`、`/area`。

| 命令 | 用途 |
| --- | --- |
| `/pa info` | 查看脚下领地信息 |
| `/pa menu` | 管理脚下领地 |
| `/pa list` | 打开可管理的领地列表，支持远程管理 |
| `/pa add 玩家` / `/pa remove 玩家` | 添加或移除成员；添加对象必须曾进入此服 |
| `/pa set 权限 true/false [成员]` | 修改访客/环境规则，或成员独立规则 |
| `/pa name 名称` | 设置领地名称 |
| `/pa item 编号 [在线玩家] [数量]` | 管理员发放核心，数量 1–64 |
| `/pa save` | 管理员立即保存 |
| `/pa reload` / `/pa load` | 管理员重载配置与存档 |
| `/pa import` | 管理员导入 `legacy-save` 中的旧数据 |
| `/pa mirror` | 管理员查看本次启动的玩家动作拦截计数 |
| `/pa status` | 管理员查看锁定状态、领地数量和框架版本 |

旧版 `welcome`、`farewell` 字段保留在存档中。占领标题改用固定样式，这两个旧命令会说明当前行为，不再写入无效显示配置。

`mirror` 为拦截计数，不是旧版纳秒耗时监控。首页书本左键打开设置、右键打开成员。点击添加成员后直接输入玩家名，无须输入 `/pa add`；输入 `取消` 或 `cancel` 退出，默认 60 秒超时。权限菜单左键切换、右键恢复默认；移除成员须 Shift 点击确认。菜单每次写入都会重新检查领地存在性与管理权限。

权限：

| 节点 | 默认 | 作用 |
| --- | --- | --- |
| `PurtmarsArea.create` | 所有人 | 创建核心领地 |
| `PurtmarsArea.command.admin` | OP | 管理命令和所有领地 |
| `PurtmarsArea.bypass` | OP | 绕过玩家动作保护，不绕过核心完整性和领地重叠检查 |
| `PurtmarsArea.bypass.<世界名>` | 不默认授予 | 在未加入白名单的世界放置核心 |

## 从 2019 版导入

1. 保存并关闭旧服，备份旧 `plugins/PurtmarsArea` 与世界。
2. 新插件使用新格式配置，**不要直接覆盖为旧 config.yml**。旧数字材质 ID 已被移除，例如 `95` 应改为 `WHITE_STAINED_GLASS`，`370` 应改为 `GHAST_TEAR`。原来所有 `Cores` 编号须在新配置中有对应定义。
3. 把旧 `save` 目录复制为新插件数据目录下的 `legacy-save`，保留 `<世界名>.yml` 文件名；世界须保持相同名称。
4. 执行 `/pa import`。成功导入使用确定性 ID，再次导入会跳过相同记录。无效数据或与已有领地重叠时整批拒绝。
5. 旧记录里的姓名会在对应玩家登录时绑定 UUID；在线玩家导入后立即绑定。应在具有正常身份校验的服务器上迁移，避免名字冒用。
6. 旧核心物品的 1.11 NBT 标签不会自动改写为现代 PDC，请用 `/pa item` 重新发放。已经放置的核心由导入后的坐标识别。

新数据以 schema 3 的绝对边界保存至 `areas.yml`，上一快照为 `areas.yml.bak`。schema 2 半径会按原来的闭区间读取，首次成功保存才转换，旧扩展核心和权限不变。加载失败不会自动用备份覆盖原文件；先检查日志，修复或从备份恢复后再重载。锁定模式禁止新的玩家操作及核心修改，避免解析错误被当作荒地。

当前没有 Realms 的区块 PDC 数据迁移器、MythicMobs 专用规则，也不承诺旧插件 Java API 的二进制兼容。原版配置中的材质筛选列表与 `item.yml` 皮肤没有逐项移植；交互类别由 26.2 API 判定，菜单按照参考截图布局。

## 构建与验证

使用 JDK 25 或更新版本：

```sh
./gradlew clean test dist
python3 scripts/verify_artifact.py
```

编译依赖固定为 Paper API `26.2.build.129-stable`，输出字节码 Java 25。交付文件 `dist/PurtmarsArea.jar` 已经 TabooLib 引导与包名重定向处理，不应把中间文件或旧版插件混用。

自动化测试覆盖完整交集、负坐标索引、成员权限、UUID 身份、旧数据、数据损坏、备份、存储失败回滚、同主人重叠拒绝、旧扩展保留、准确边长、破碎核心标记、菜单布局分页、私聊会话失效、放置取消回滚，以及保护事件。测试使用 26.2 API 和模拟对象，**没有启动 Minecraft 服务端，也没有真实玩家 GUI/游戏验收**。

上线前在测试服至少验证：主人/成员/访客三种身份放置与拆除、成员授权、双箱跨界、活塞推拉、漏斗偷取、水桶与爆炸、车辆进入、跨世界传送、菜单快速点击、重启后领地仍受保护，以及原版插件共存时的事件取消顺序。实际报告见 `VALIDATION.md`。

不支持 Folia；`plugin.yml` 明确声明 `folia-supported: false`。本项目针对 Paper 26.2，不把 Spigot 或其他 Minecraft 版本列为已支持平台。

## 参考与实现边界

参考 [Bkm016/Realms](https://github.com/Bkm016/Realms)（提交 `d661bd0d3a9742ff10ae2edbc886a6c70ebc5541`）的核心扩展、出入提示和权限结构，以及用户提供的 PurtmarsArea 1.2 JAR 的配置、命令和数据格式。实现为独立新代码，使用现代 Paper API 与 PDC，不打包旧 TabooLib、旧版 NMS、Realms 源码或反编译文件。

## 发布与许可证状态

仓库：[PigeonMoe/PurtmarsArea](https://github.com/PigeonMoe/PurtmarsArea)。成品从 [Releases](https://github.com/PigeonMoe/PurtmarsArea/releases) 下载。

2026-10-08 核对：参考项目 Bkm016/Realms 未包含 LICENSE 文件，GitHub API 的 license 字段为空。因此本项目按其当前状态发布，不另外指定根项目开源许可证，也不声称 Realms 使用 MIT、GPL 或其他许可证。参考代码的公开可见性不等同于已授予许可证。

第三方组件保留各自许可：TabooLib 为 MIT，许可全文随 JAR 放在 `META-INF/licenses/TabooLib-LICENSE.txt`；Gradle Wrapper 为 Apache-2.0，见 `gradle/wrapper/LICENSE`。详见 `THIRD_PARTY_NOTICES.md`。
