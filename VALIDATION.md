# 验证报告

日期：2026-10-08（Asia/Shanghai）。

## 结果

- 26.2 API 编译、TabooLib 引导打包和交付复制通过。
- 自动化测试 **86 项全部通过**，无失败、错误或跳过。
- JAR ZIP 完整性、入口类、API 版本、Folia 声明、Java 字节码与框架版本检查通过。
- **未启动 Minecraft 服务端；未进行真实玩家、实际 GUI 或原服数据迁移验收。**

| 测试组 | 数量 | 失败 | 跳过 |
| --- | ---: | ---: | ---: |
| CoreServiceTest | 15 | 0 | 0 |
| GeometryTest | 19 | 0 | 0 |
| PermissionsTest | 31 | 0 | 0 |
| ProtectionTest | 15 | 0 | 0 |
| StoreTest | 6 | 0 | 0 |

构建执行：先完成 `clean test dist`，补充实体放置、骑乘移动与核心拆除测试后，再执行 `test dist`；最终交付校验使用 `python3 scripts/verify_artifact.py`。

## 交付

文件：`dist/PurtmarsArea.jar`。

SHA-256：`7623baf3fc42385018c0be72d37c5e147275cb841929ff87bf36b001a5fee94c`。

结构化报告：`dist/verification.json`；校验和：`dist/SHA256SUMS`。

依赖：TabooLib `6.3.0-0e3a911`、TabooLib Gradle 插件 `2.0.39`、Paper API `26.2.build.129-stable`、Kotlin `2.3.20`。本次在 JDK 26.0.2.1 上编译与测试，应用字节码目标为 Java 25（class major 69）。

构件版本依据：2026-10-08 读取官方 Maven 元数据，TabooLib 最新非测试版本为 `6.3.0-0e3a911`，其后为测试构建；同时核对官方源码 HEAD 为 `0e3a911fc55624075b5c9abd4368cb5b063b022b`。代码与交付包均固定同一版本。

## 已验证的主要行为

- 完整三维交集与边界，包含无角点包含的十字重叠、负坐标、世界隔离、扩展增删与索引清理。
- 主人 UUID、成员独立默认值、成员显式覆盖、旧姓名兼容，访客开关不会改变成员自己的缺省规则。
- 多核心数据、权限、中文名称和提示可读写回放；空存档、备份、无效数据和旧格式导入。
- 创建数量、扩展数量、世界权限、禁止侵入他人领地、禁止无提示合并多个领地。
- 其他插件取消放置时撤销领地；保存失败回读最近成功存档；主核心/扩展核心的拆除和保存。
- 方块、水桶实际目标、传送两端、移动、液体两端、活塞跨界、核心防移、爆炸过滤、发射物来源、实体放置、骑乘越界。
- 事件处理器的静态 HandlerList 检查；未注册抽象 PlayerBucketEvent。

## 验收边界

模拟对象验证事件决策，不模拟完整 Minecraft 物理、客户端预测和其他插件的实际事件顺序。首次 TabooLib 依赖下载、插件在真实 Paper 启动中的加载、菜单操作、粒子观感、复杂红石、跨世界载具与真实旧世界转换，仍需要测试服验收。

支持范围为 Paper 26.2，不含 Folia。Realms 用作行为参考，不包含其区块 PDC 导入、MythicMobs 专用接口或旧 Java API 的二进制兼容；详见 README。

发布前补充了 TabooLib MIT 许可全文及 Gradle Wrapper Apache-2.0 许可，并重新打包和校验；业务源码未改变。
