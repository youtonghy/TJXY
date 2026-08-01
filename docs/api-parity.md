# API / 功能兼容矩阵

> 计划基线：`PLAN.md` v2.6
> 契约：钉扎 Jellyfin OpenAPI 12.0.0
> 图例：`✅` 计划内 · `⚠️` 最小/待客户端验证 · `⬜` 未实现 · `❌` 非目标

当前自动化实测级别：L0 发现、L1 登录、L2 目录浏览、L3 Direct Play/UserData、
L4 VirtualFolders/Tasks/Admin 核心链路，以及 L5 Search、WebSocket、Collections、
Playlists、DisplayPreferences、Devices 与 API Keys 最小兼容均已有 HTTP/TCP 契约覆盖；真实第三方客户端
仍按非阻塞 observation 单独记录。
L2 当前策略是所有已认证且未禁用的用户可见全部启用媒体库；尚无细粒度媒体库 ACL。
发布门禁：钉扎 Jellyfin OpenAPI/HTTP 契约；不固定客户端应用版本。

TJXY Web 另有同源自用接口：`GET|PATCH /Users/Me/Profile`、
`POST /Users/Me/Password`、`GET /Users/Me/Insights`、`GET /Discover/Popular`、
`GET /Discover/Tmdb/Popular` 与 `GET /Discover/Server/Top`。这些接口要求正常用户
session，不冒充其他用户；TMDB token 只在服务端解密，排行榜按 UTC 日期进行进程内
日缓存，刷新失败时使用最近一次成功结果。

---

## 1. 客户端最小链路

| # | 行为 | 路由 | v2.6 语义 | 状态 |
|---|------|------|-----------|------|
| 1 | 发现 | `GET /System/Info/Public` | 诚实 ProductName/Version | ⚠️ 最小 DTO 已实现，待真实客户端验证 |
| 2 | 登录 | `POST /Users/AuthenticateByName` | canonical MediaBrowser header + aliases | ⚠️ Argon2id 与持久到撤销的 session 已实现，待真实客户端验证 |
| 3 | 当前用户 | `GET /Users/Me` | SQL SoT | ⚠️ token digest 查询已实现，待真实客户端验证 |
| 4 | 能力 | `POST /Sessions/Capabilities/Full` | DeviceProfile 参与 Direct Play 判断 | ⚠️ Full 与 legacy capability adapter 已持久化当前 session，并参与 PlaybackInfo 容器门禁；代表性请求方言矩阵待补齐 |
| 5 | 首页 | `GET /UserViews` | Redis 预热，miss 回源 SQL | ⚠️ SQL SoT、generation/user-revision cache-aside、miss/损坏/断连回源已实现；Redis 启用时 ready 后异步预热最多 128 个启用用户，失败只记录且不阻断 ready；ACL 未实现 |
| 6 | 浏览 | `GET /Items` | 未展开 Series 可触发高优先级 Expand | ⚠️ 根视图、父项、active publication 递归读取、类型过滤、稳定分页、generation/user-revision cache-aside、高优先级协调及递归 SQL Structure worker 已实现；更完整命名分类未实现 |
| 7 | 主页行 | Latest / Resume / NextUp | SQL + Redis user revision | ⚠️ Latest、Resume、NextUp 的 SQL 可见性、用户隔离、稳定排序/分页及 cache-aside 已实现；启动预热覆盖全局 Latest、最多 64 个 Library Latest 以及默认 Resume/NextUp；NextUp 高级筛选/重看模式未实现 |
| 8 | 详情 | `GET /Items/{id}` | 不触发 Media Probe | ⚠️ 鉴权、用户边界、active publication 读取、DTO、cache-aside 及 Movie Source Index enqueue/join/有界等待和生产 worker 已实现；更完整 fields 扩展未实现，且不会 Probe |
| 9 | 图片 | `GET /Items/{id}/Images/{type}` | 内容寻址 AssetBlob | ⚠️ 鉴权原图 GET/HEAD、ImageTags、ETag/304、受限解码/原子内容寻址写入及 TMDb Primary 采集已实现；本地图/import 下载器与变换未接入 |
| 10 | 播放信息 | `GET|POST /Items/{id}/PlaybackInfo` | 多 MediaSource；首次可惰性 Probe | ⚠️ 鉴权、DeviceProfile 容器门禁、多 active source、Probe single-flight、catalog/user/probe-revision 隔离的 source-metadata cache-aside 及 Filesystem Matroska/ISO-BMFF worker 已实现；Filesystem 与 provider-neutral 云端请求/完整规范化响应 golden 已固定，云端 deterministic contract 通过真实 Source Index、双源 Probe、Admin 默认策略和 TCP 验证完整有序列表。完整 stream 字段仍待补齐 |
| 11 | 原文件 | `GET|HEAD /Videos/{id}/stream` | 本地读取或云盘统一 Range 代理 | ✅ active source/location 鉴权、Filesystem/云端 provider-neutral 原文件 GET/HEAD、单 Range、206/416、ETag/If-Range、字节一致及上游身份隐藏已验证 |
| 12 | 外挂字幕 | 两种 `/Videos/{id}/{mediaSourceId}/Subtitles/.../Stream.{format}` | 鉴权、本机路由、仅源格式 byte-for-byte | ⚠️ 两种路由、活动发布/索引/库授权、原格式直出已实现；转换与非零时间偏移明确拒绝，待真实客户端验证 |
| 13 | 进度 | Sessions Playing/Progress/Stopped/Ping | SQL SoT + Redis 热点刷新 | ⚠️ 持久 playback session、Playing/Progress/Stopped 幂等写入与 Ping 活跃刷新已实现；前进进度按心跳墙钟时间封顶累计 `watched_ticks`，快进、倒退与异常时钟不会虚增观看时长；Jellyfin OpenAPI 的可选 `ItemId`/`MediaSourceId`/`PlaySessionId` 可被兼容接受，无 item 的遥测 no-op，item-only 事件选择首选 source 并派生稳定 session；Redis、完成阈值和真实客户端验证未实现 |
| 14 | UserData | `GET|POST /UserItems/{id}/UserData`、Favorite/Played | 绑定稳定 CatalogItem ID | ⚠️ UserItems GET/字段级 POST、Favorite/Played POST/DELETE、同事务库授权锁与 user revision 已实现；Redis 与真实客户端验证未实现 |
| 15 | 实时刷新 | `GET /socket` | `LibraryChanged` / `UserDataChanged` | ⚠️ 认证 WebSocket、进程内有界广播、完成 durable cache invalidation 后的 `LibraryChanged` 及仅目标用户可见的 `UserDataChanged` revision 事件已实现；断线客户端通过 SQL revision 重读，跨实例消息投递与真实客户端验证未实现 |

---

## 2. 系统、认证与用户

| 能力 | 计划 | 状态 |
|------|------|------|
| System Info / Public / Ping / Endpoint | ✅ | ⚠️ Public、Ping、health 及认证 `GET /System/Endpoint` 已实现；Endpoint 仅依据传输层 peer 判断 loopback/private/link-local 网络，不采信可伪造的转发头；待真实客户端验证 |
| Branding | ✅ | ⚠️ 公开 `GET /Branding/Configuration` 默认 DTO 与 PascalCase golden 已实现；自定义免责声明/CSS 的 Admin 持久化未实现 |
| Startup Wizard | ✅ 最小 | ⚠️ 是否已有用户已反映到 Public DTO；交互式向导未实现 |
| AuthenticateByName | ✅ | ⚠️ Username/Pw、统一 401、持久 session 已实现；待客户端验证 |
| canonical `Authorization: MediaBrowser` | ✅ 发布门禁 | ⚠️ Client/Device/DeviceId/Version、Token 与 `ApiKey` query 已实现；待客户端验证 |
| legacy X-Emby/X-MediaBrowser aliases | ✅ | ⚠️ X-Emby-Authorization、X-Emby-Token、X-MediaBrowser-Token、`api_key` 已实现，可配置关闭 |
| Users / Me / Admin CRUD | ✅ | ✅ Me、启动时首管理员及鉴权 Admin list/get/create/rename/password/policy/delete 已实现；策略持久化限于 TJXY 支持的 administrator/disabled，本地 provider 与 Direct Play-only 能力不可被请求改写；所有敏感变更撤销旧 auth revision，且禁止移除最后一个 enabled admin |
| DisplayPreferences | ✅ 最小 | ✅ 认证 `GET|POST /DisplayPreferences/{id}` 已实现；非 UUID ID 使用 Jellyfin UTF-16LE MD5 GUID 兼容映射，偏好按 user/display/client 原子替换并持久化，跨用户访问返回 403，DTO 默认值与 PascalCase golden 已固定 |
| HeroUI Admin 登录与 Users CRUD | ✅ | ✅ 同源 `/admin/` 生产构建采用 `ra-core` + HeroUI v3，登录、用户列表/创建/改名/密码/策略/删除与移动端布局已实现，并有完整 Playwright 生命周期门禁 |
| HeroUI Admin Libraries | ✅ | ✅ 管理员可列出、创建空库、重命名和删除 Library，并以 `profile_version` CAS 编辑 Full/Lazy/Hybrid/Manual 或完整四项 effective policy；可分页查看各库的持久化 Series 候选，仅为 enabled `background` 库新增 pin，并可清除 dormant pin；表格仅显示 root 数量且不暴露路径。生产 Playwright 覆盖 Library/策略 SQL 生命周期与候选真实空页，候选 pin/unpin SQL 生命周期由 server 集成契约覆盖 |
| HeroUI Admin Tasks | ✅ | ⚠️ 管理员可启动/取消 Full Media Scan、查看有界 newest-first durable job 安全状态，并按现有 Library root 或 CatalogItem 显式提交 Validate/Discover/root Full/Resolve/Expand/Index/Probe；root Full 使用 Library-root binding scope，shared root 不跨库推进 Discover。原始错误、lease、路径与凭据不出站；日志摘要与缓存状态未实现 |
| HeroUI Admin cloud Storage | ✅ | ⚠️ 管理员可选择目标库、启动服务端 OAuth、确认回调、选择 My Drive 或分页 Shared Drive、通过服务端 OAuth-session UUID cursor 完整分页并逐层选择 Google/OneDrive Personal 目录、提交绑定；确定性 fake-provider/HTTP 契约已覆盖追加、去重、空页续翻、失败重试和上下文/owner/state 隔离，但尚非 live Google/Microsoft 验收；Storage 状态/重授权、metadata、迁移及冲突管理页面未实现 |
| HeroUI Admin Access | ✅ | ✅ 已认证的 `/admin/access` 以 Devices/API Keys 标签页提供设备改名、确认撤销、API key 创建/遮罩/显示/复制/确认删除；权威重载可取消并防旧响应覆盖，密钥不写 Web Storage 或诊断产物。生产 Playwright 已覆盖持久化恢复、API key 鉴权、撤销/删除失效，以及桌面、768px 和 390px 布局 |
| HeroUI Admin Dashboard | ✅ | ✅ `/admin/` 首页通过仅管理员可访问的 Summary、NowPlaying、LoginHistory、WatchHistory API 展示用户、影片、电视剧、剧集、播放趋势、Top 10、60 秒内在线会话和分页活动记录；时间范围限制为 31 天，登录记录仅包含成功建立的会话 |
| API Keys / Devices / Sessions | ✅ 最小 | ⚠️ 登录 session 与 capabilities 已持久化；认证 `GET /Sessions` 支持管理员全局/普通用户本人范围及 device、recent、controllable 过滤，`POST /Sessions/Logout` 会立即撤销当前 token。管理员 `GET|DELETE /Devices`、`GET /Devices/Info` 与 `GET|POST /Devices/Options` 已实现；DeviceId 以 SHA-256 natural key 保持跨数据库大小写精确语义，列表在 SQL 中先按设备选择最新活跃 session 再应用 256 项上限，options 更新与批量删除锁定相关活跃 session，删除会先全量校验再原子撤销。`UserId` 会校验用户存在；当前尚无设备 ACL，因此已有用户仍可见全部活跃设备。API Keys 后端已完成：管理员 canonical `GET|POST|DELETE /Auth/Keys`、256 项全局上限、digest-only 鉴权索引、版本化 AEAD 可恢复密文、用户 revision 变更同事务物理撤销、全响应 `no-store` 与 fail-closed 启动校验均已由 SQLite/PostgreSQL 17/MySQL 8.4、HTTP/TCP 和重启契约验证；Devices 仍待真实第三方客户端验证 |
| Quick Connect | ❌ v1 | ❌ |

---

## 3. Library、Storage 与 Tasks

| 能力 | v2.6 计划 | 状态 |
|------|-----------|------|
| VirtualFolders CRUD | Library + StorageRoot + 持久化 effective ScanProfile | ✅ 管理员 GET、空库或单 Filesystem root POST、按无歧义精确名称 DELETE/重命名、opaque StorageRoot locations、通用 root 解绑、预设/高级 LibraryOptions 更新及 profile_version CAS 已实现；Filesystem 创建原子持久化 canonical root/account/root object/membership/初始 sync，最后解绑禁用 runtime 但保留 StorageObject/UserData，同路径同 identity 重绑复用历史对象；管理事务串行防止重名，import 引用返回 409 |
| FilesystemBackend | 正式 | ⚠️ 稳定身份/revision、有界 Range、多持久化 root 重启装载、新绑定同进程热激活、account-scoped inventory runner、native recursive event monitor、500ms quiet-window 调度与显式递归 Validate 已实现；事件仍按 hint 处理，平台丢事件由显式 Validate 校准 |
| Google Drive 原生 backend | My Drive + Shared Drive | ⚠️ Files/Changes/Shared Drive/OAuth refresh/严格 Range/429 Retry-After、服务端 authorization-code + S256 PKCE、一次性 session-bound state、Google `about.user` 账号身份、分页 Shared Drive 枚举、My Drive/Shared Drive 目录 UUID-cursor 分页选择、版本化 AEAD credential loader、同进程 runtime 热激活、Admin 向导及管理员初始绑定/cursor 已实现；原始目录 page token 不出服务端，Shared Drive 列表仍沿用既有合约；在线重新授权/轮换命令未实现 |
| OneDrive 原生 backend | **仅 Personal** | ⚠️ Files/children/Delta、OAuth refresh-token 加密轮换、严格 Range、bearer-safe 临时 URL、runtime loader/worker 与同进程热激活、服务端 authorization-code + S256 PKCE、一次性 session-bound state、Microsoft Graph 身份/Personal drive/root 推导、完整 `@odata.nextLink` 的 configured-origin 校验与服务端 UUID-cursor 目录分页、Admin 向导及管理员初始绑定/root/delta cursor 已实现；Graph URL 不出 Admin；在线重新授权/轮换命令未实现 |
| OneDrive Business / SharePoint | ❌ v1 非目标；模型可留枚举，绑定 4xx | ✅ adapter 与 OAuth 回调发现均明确拒绝；旧的浏览器直传凭据绑定路由已移除 |
| rclone/FUSE 强制依赖 | ❌ | ❌ |
| Google Changes | 增量对象同步；removed 才确认缺失 | ⚠️ opaque cursor CAS、多页终态、已物化 parent 下新增/重命名/双边 move 投影、移入未物化 parent 时旧 relation 转 TemporarilyUnavailable、removed ConfirmedAbsent、outbox 水位、周期 worker 及 410 后独立 root-scoped Strict Lazy 恢复已实现；恢复使用独立 RecoverStorageCursor natural key，完成后使深层 scope 按需重建，终止失败进入 RecoveryFailed，审查后可通过 application recovery API 原子创建新 job 并恢复；管理员可显式运行递归 Validate/repair |
| OneDrive Delta | 增量对象同步；deleted 才确认缺失 | ✅ 初始 latest cursor、nextLink/deltaLink、多页 cursor、parentReference move、deleted ConfirmedAbsent、独立 root-scoped 410 恢复及 durable worker 已实现；同一 Delta 响应重复出现的对象按 Microsoft Graph 语义只应用最后一次出现 |
| Storage presence | Present / TemporarilyUnavailable / ConfirmedAbsent | ⚠️ root-local Present、未物化目标 move、inventory/Validate 枚举及媒体/字幕/Probe/NFO 普通 get/open-range 遇到单次 404、限流或临时后端失败时的 TemporarilyUnavailable、provider removed/deleted、完整 scope 与递归 Validate 未观察项的 ConfirmedAbsent 已实现；普通读取响应前及流中错误均写 sanitized reason、逐 root revision/outbox 并投影，成功 get/open 恢复 Present，客户端取消不误标失败；incremental move/remove 与 completed inventory 会在同一 revision 级联失效已物化后代，播放和外挂字幕读取会实时复核 root-local 祖先链；播放优先 Available、无健康副本时允许 transient 重试，Probe 仅选仍有 live root relation 的对象；Validate 最终扫尾会级联确认缺失目录当前仍挂载的后代 |
| Google 初始对象同步 | **Strict Lazy**：标题层 inventory；访问时 scoped sync；禁止默认全树 Inventory First | ⚠️ 绑定事务原子创建根目录的非递归 Scoped Storage Sync 并返回 `InitialSyncJobId`；完成后 durable Discover 从已协调 SQL 根层发布轻量 CatalogItem；cursor 失效时暂停增量、重建同一标题层并由 recovery job 原子激活 fresh cursor |
| Scoped Storage Sync | Strict Lazy 先物化 SQL 子树，Media Scan 不直连 backend | ⚠️ 请求侧按 matched StorageObject enqueue/join、root affinity 纳入 durable natural key、分页原子提交、非末页强制 `children_indexed=false`、末页原子推进 children/relation revision、inventory reparent 同 revision 写旧 parent `MovedOut` 与新 parent `Upserted`、按 claim attempt 生成重放 identity、整轮直接子项对账、result/children/reconciled 三重门禁、Filesystem/account/provider-drive worker 及 encrypted credential 自动装载已实现；旧任务升级时可唯一恢复 root 的继续执行，无法唯一恢复的 live job 明确失败；显式 Validate 使用独立 root job 递归复用该流水线，retryable 枚举失败会提交 scope-local transient presence 并允许成功重试恢复 |
| Storage change reconcile | 列级 outbox、dedupe/lease/重放；连续 reconciled sync watermark | ⚠️ lease fencing、失败退避、并发 drain join、独立常驻 backlog reconciler，以及 Location availability、Probe Stale、item tombstone、Matched Item revision、active Structure scope 下 owner/Season/Episode revision 传播、同批连续事件失效、generation 与同事务 durable cache invalidation 已实现；对象事实持久记录来源 root，Metadata/Expand/Index/Discover snapshot、Metadata commit 及 Source/Structure pointer switch 均按具体关系/对象 revision 复核，授权与协调绑定同一 root/object pair，且无关 root pending revision 不阻塞；升级迁移会回填唯一 root 的旧事实，将跨 root 歧义事实 fail-closed 隔离、使父 scope 重新物化并推进相关 Catalog 修订；该恢复是单向数据修复，升级必须先停止旧写入实例，单步 down 不还原歧义事实；Redis 旧 generation 通过 TTL 注册表有界删除，但 Initial/incremental/scoped/Validate 到受影响 Item/PlaybackInfo 的完整场景矩阵仍待补齐 |
| Filesystem events/additions | 稳定 file ID 配对 move；弱路径只生成 relink 候选 | ✅ file-events capability 与 provider Changes 分离；native watcher quiet-window 后只为已物化父目录 enqueue/join durable scoped inventory，稳定 file ID 复用对象并更新父关系；`path_weak` 保留新旧对象并以 size/mtime/name/checksum 证据写 Pending Admin relink candidate；管理员可分页查看脱敏候选并以 CAS 幂等 Confirm/Reject，确认后复用旧 CatalogItem/MediaSource presentation identity 且保留 UserData；有界队列溢出会明确失败并重启 watcher，平台丢事件按计划由显式 Full Validate 校准 |
| Full / Lazy / Hybrid / Manual | effective policy 写 SQL，Admin/调度器/重启后一致 | ⚠️ 四预设 domain 映射、SQL round-trip、VirtualFolders 管理读写及 profile_version fence 已实现；refresh/自动 Discover 读取四项 effective policy 而非 profile 名，Full 对每个 root 做 durable 递归 Validate，Lazy/Hybrid 仅做 root 非递归 Scoped Sync，且父扫描跨 retry 固定同一个 root prerequisite；只有 eager 才 Expand/Index/Probe，`title_layer` 不会在后台 Structure 发布后把新子项吸入同次扫描，`all_synced_objects` 仍会处理投影子项；basic/full 请求等级、完成水位和同 natural key 单调升级已持久化，运行中 Basic 被升级时旧发布会回滚并以 Full 重试，FullScan 不再把同 revision 的 Basic 当作 Full；Source publication 取所属启用媒体库的最高要求，none 不隐式 Resolve，显式 Admin/root Full 固定请求 Full；Partial 推进的是已尝试等级而不会使扫描失败。Manual 显式 Expand/Index 已实现 sync-first durable continuation，投影 Episode re-index 以 effective source Location 为锚点且不会吸入同 Season 的其他视频；Hybrid refresh 会以 SQL 级上限选择未展开 Series，并把父任务本次候选持久化为跨 retry 固定批次，按管理员 pin、观看中、同用户在 active publication 内“已有已看且仍有未开始 Episode”的首页 NextUp、收藏、最近添加排序并以低优先级复用 Expand single-flight；Structure publication 持久化 root-local Season/Episode scope，root Full 可读取投影 Season NFO 并原子发布 Episode Source；真实服务 TCP smoke 已验证 Hybrid `/Library/Refresh`、Manual 显式阶段及 root Full 的 Validate、Discover、Metadata、Expand、Probe、Browse、PlaybackInfo 全链路，且 root Full 不重复调度 Index；Admin 可分页 pin/unpin，策略切换后偏好 dormant 且不取消已提交任务；生产 scheduler 默认每 900 秒以最低优先级提交同一 policy-aware durable refresh，首轮延迟且错过 tick 不追赶，可通过环境变量改周期或禁用。在线 provider 的 Full 关联字段及版本化完成证据仍未实现 |
| Storage Tasks | Inventory/Changes/Delta/Auth/Validate | ⚠️ 持久化 scoped inventory、独立 root-scoped Validate WorkJob/runner、SQL-cursor Google Changes/OneDrive Delta worker 已实现；独立 Auth 运维任务未实现 |
| Media Tasks | Discover/Resolve/Expand/Index/Probe/Full/Validate | ⚠️ durable SQL Discover、NFO/provider Resolve、Expand、Index、Probe、Library Full 与 binding-scoped root Full worker 已实现；Full 持久记录子任务、传播失败、等待发布水位并在取消时终止由该扫描创建的子任务；Admin 可显式提交 root Validate/Discover/Full、CatalogItem Resolve/Expand/Index，以及仅针对 active/available Source 的 CatalogItem Probe |
| ScheduledTasks API | ✅ | ⚠️ Full Media Scan 的 list/detail/start/cancel、`Library/Refresh` 按 persisted effective policy durable enqueue、生产 worker、bounded newest-first `Admin/Tasks/Jobs` 安全状态及 scoped Validate/Discover/Resolve/Expand/Index/Probe Admin 命令已实现；观测 DTO 不暴露持久错误、lease 或 storage secret；显式 Expand/Index 返回最终 media job，未物化 scope 通过 deferred sync dependency 等待真实 revision，依赖失败转为脱敏终止失败；显式 Probe 以高优先级原子 enqueue/join 最多 256 个可用 active Source，空、不可用或超限 Source 集合返回 409 且不隐式 Index；Discover 进度按 library-root 绑定隔离并以 profile_version 阻止旧策略发布；其他任务类型未实现 |

---

## 4. Catalog 与浏览

| 能力 | v2.6 语义 | 状态 |
|------|-----------|------|
| CatalogItem 与路径解耦 | ItemId 不由路径决定 | ⚠️ 领域类型、schema、active publication 详情/播放读取及 storage relink 后稳定 ItemId 已实现；跨来源自动 identity resolution 尚未完整接入 |
| 跨库 CatalogItem 复用 | `library_catalog_items` 多对多 | ⚠️ 查询以 membership `EXISTS` 校验并防止跨库子项泄漏；写侧尚未实现 |
| 多 MediaSource | 完整多源 DTO + §4.4 正式默认排序；客户端版本 UI 非门禁 | ⚠️ publication-owned SQL 投影、active-only 完整 DTO、DeviceProfile/上次使用、管理员默认与优先级、分辨率、编解码兼容、账号状态和 stable-key 默认排序均已实现；账号状态当前以 `Active > Ready > 其他` 表达，尚无运行时认证/限流健康模型 |
| MediaSource re-index | 稳定对象/content identity/legacy mapping 保留对外 ID | ⚠️ 相同 stable identity 的 re-index 保留 MediaSourceId、presentation key、Probe 状态与字幕 delivery index；Filesystem 真实进程 TCP smoke 与 provider-neutral 云端 real-TCP contract 都会在 replacement publication 前后重复完整 PlaybackInfo/交付链路，断言 publication ID/generation 前进而两源顺序、默认策略、presentation、URL、字幕 index 与原字节稳定。管理员确认的 PathWeak relink 会建立 durable identity alias 并让 replacement 复用旧 MediaSourceId/presentation key；source removal/tombstone、pointer switch 并发读取、通用 content identity 与 alias 管理未实现 |
| 多 MediaLocation | 一个版本多个镜像 | ⚠️ publication relationship、全局 StorageObject identity、动态 presence、可用 location 选择及 provider-neutral 代理已实现；可信 content identity 跨镜像复用与完整健康排序未实现 |
| StorageObject 稳定身份 | provider ID/可靠 file ID；Filesystem 路径 fallback 标为 weak | ✅ provider stable ID、Unix dev/inode 与非 Unix canonical-path `PathWeak` 已进入持久化 identity_quality；稳定 ID rename 保持对象身份，弱身份只生成待确认 relink candidate |
| Items query/filter/sort/page | 索引 SQL + Redis cache-aside | ⚠️ SQL 类型过滤、`SortName` 升序、1..=200 分页及 cache-aside 已实现；其他排序未实现 |
| UserViews / Latest / Resume / NextUp | 首页预热 | ⚠️ 四个 SQL-authoritative 路由及 generation/user-revision cache-aside 已实现；Redis 启用时 ready 后预热最多 128 个启用用户的 UserViews、全局 Latest、最多 64 个 Library Latest、默认 Resume/NextUp；NextUp 高级筛选/重看模式未实现 |
| Lazy 初始基础 metadata | title/year/overview/provider/Primary | ⚠️ 根层 Discover 生成 title/year，Resolve 支持 NFO、可选 TMDb、Naming fallback 与 Movie/Series Primary 本地化；Structure 投影 Season 已可从持久化 storage scope 读取 NFO，Season/Episode 在线 provider parent-aware 查询和本地图仍未接入 |
| Lazy Movie 首次展开 | 详情触发 Source Index；PlaybackInfo 可等待同一任务；成功 bump generation | ⚠️ 详情 enqueue/join/有界等待、SQL inventory 分类、外挂字幕关联及 Source 原子发布 worker 已实现；更完整容器/命名分类未实现 |
| Lazy Series 首次展开 | publication staging 后一次切换全部 Season/Episode；子 Episode source 已 Indexed | ⚠️ 递归 scoped inventory 调度、确定性子项 ID、Episode Source/Location/Subtitle 与单一 Structure pointer 原子发布 worker 已实现；更完整季/集命名解析未实现 |
| Expand single-flight | 先等待 sync/result/reconciled revision，再由多实例 join 持久化 leased job | ✅ 请求协调器、sync-first 编排、持久化 join/fencing、递归 inventory retry 及 Structure worker 已实现 |
| 大型 Series 可见性 | 分批 staging，查询只读取 active publication | ⚠️ 分批投影、完整 manifest/拓扑 seal、生产 worker、短事务 pointer/generation 切换与 active-only 查询已实现；旧 publication GC 未实现 |
| 跨库子树 membership | 展开子项继承父 Series 当前全部库关联 | ⚠️ active publication 查询及图片授权动态继承 owner 的全部启用库关联；写侧/ACL 尚未实现 |
| 空未展开结果缓存 | 短 TTL | ✅ Items/Home/detail 的空结果使用独立短 TTL，非空结果保留正常 TTL；契约测试锁定 3 秒与 300 秒边界 |
| Search | P1.5 + Redis | ✅ `GET /Search/Hints` 已认证、按可见性筛选、支持类型筛选和稳定分页；结果缓存按 catalog/user revision 隔离，搜索词仅进入摘要键 |
| Manual root scope | 阶段命令必须显式携带 root 或 CatalogItem scope | ⚠️ `ValidateStorage/{rootId}`、`DiscoverTitles/{rootId}`、`FullScan/{libraryId}/{rootId}`、`ResolveMetadata/{itemId}`、`ExpandItem/{itemId}`、`IndexMediaSources/{itemId}` 与 `ProbeMedia/{itemId}` 管理员命令及 Admin 控件已实现；root Full 使用独立 task kind 与 Library-root binding scope，固定 Full command policy，并把所选 root affinity 持久化为派生 Resolve/Expand/Index job 的执行输入与提交围栏；CatalogItem publication 仍全局 single-flight，不同 affinity 的同 revision 请求不兼容；Expand/Index 对未物化 scope 持久返回最终 media job，依赖 Scoped Sync 完成并对账后才原子捕获 input revision/领取 lease，依赖失败不会永久 Pending；投影项持久保存经过 owner/root 授权的 storage scope，Season metadata 与 Episode source 可沿相同 scope/revision fence 工作；旧库升级会退役缺 scope 的 live Structure publication、推进 owner revision 并强制重新 Expand，此数据迁移要求先停止旧版本实例，不能与旧写入路径滚动并行；投影 Episode 可使用 effective source Location 的父目录完成显式 re-index，Probe 会选择最新 effective publication；Probe 可显式重跑已 Probed Source，无 active Source 时返回 409 而不隐式 Index；effective `library_roots/none/manual/on_playback` 不生成全局 refresh 或隐式 Discover。真实 TCP 已覆盖全部显式阶段及 root Full Series 全链路，并断言 Full 不重复调度 Index |
| Collections / Playlists | P2 | ✅ 私有 Playlist 支持创建、列表、重命名、删除、追加、认证读取及按稳定条目 ID 重排；共享 Collection 支持认证列表/读取，且仅管理员可创建、重命名、删除和追加。两者均动态过滤不可见 CatalogItem。 |

---

## 5. Metadata、身份与图片

| 能力 | v2.6 语义 | 状态 |
|------|-----------|------|
| 结构化 metadata | 全部写 SQL | ⚠️ title/original title/year/overview、Provider IDs、metadata state、请求等级/尝试水位与 generation 通过单事务发布；Basic/Full 调度和竞态已区分，Full NFO 会原子替换已评估的 People/Genres/Studios，Basic 不触碰关联；在线 provider 的 Full 关联获取、关联 provenance 与版本化完成证据尚未接入 |
| NFO / 本地图 | 导入来源，不是运行时 SoT | ⚠️ 有界 NFO parser、管理员导入、SQL direct-child sidecar 选择、snapshot/commit 双重 scope/fact revision fence、平铺 Episode 按 active video Location 同 stem 精确关联且不借用兄弟/目录级 NFO、bounded 读取与 durable 发布已实现；直接管理员导入会串行合并并推进 metadata revision，使更早领取的 resolver 无法覆盖新结果；本地图发现未实现 |
| 在线 provider | v1 仅 TMDb，可选；失败不失败整次任务 | ⚠️ 默认关闭、显式 bearer token/language、bounded Movie/Series search、固定 host 的 bounded Primary 下载与 warning 降级已实现；Season/Episode parent-aware 查询未实现 |
| Provider identity | 可存 TMDb/TVDB/IMDb 等键；在线解析以 TMDb 为主 | ⚠️ NFO/Emby/TMDb identity 可原子写入，部分来源合并保留未提及 identity；跨来源自动复用/冲突队列未实现 |
| Jellyfin/Emby metadata 插件宿主 | ❌ | ❌ |
| 弱匹配 | 只生成候选，不强制合并 | ⚠️ Filesystem PathWeak rename 候选、脱敏 Admin 队列、过期校验及 CAS Confirm/Reject 已实现；title/year/provider 跨来源候选尚未实现 |
| metadata provenance | 字段来源可追踪 | ⚠️ 基础字段与 Provider ID 保存 provider/reference/value SHA-256，来源切换按字段替换且重放不 bump generation；关联字段 provenance 未实现 |
| AssetBlob SHA-256 去重 | 相同图片只存一份 | ✅ 格式/MIME/大小/像素/解码内存限制、secure openat 原子写、并发-safe digest upsert 与跨条目去重已实现 |
| ItemAsset 引用 | 图片不归属媒体目录 | ⚠️ metadata 文本、TMDb Primary 引用及 WorkJob 结果同事务提交，支持原子替换、单次 generation bump、priority-zero ImageTags 与授权原图解析；本地图/import 采集器尚未接入 |
| Admin merge/split/rematch | ✅ | ⚠️ Filesystem PathWeak storage relink 的 list/confirm/reject 已实现，并保留稳定 CatalogItem/UserData/MediaSource identity；通用 CatalogItem merge/split 与 provider rematch 尚未实现 |
| Chapter images / Trickplay | ❌，需要视频帧处理 | ❌ |

---

## 6. PlaybackInfo、Probe 与 Stream

| 能力 | v2.6 语义 | 状态 |
|------|-----------|------|
| PlaybackInfo GET + POST | ✅ | ⚠️ 鉴权 GET/POST、可选 session/request DeviceProfile、query 覆盖 body 的 UserId/MediaSourceId/EnableDirectPlay 与安全空降级已实现；其余可选选择字段待通用契约矩阵验证 |
| Browser playback tickets | ✅ | ✅ `POST /Items/{itemId}/PlaybackTicket` 签发登录会话/媒体源/播放会话绑定的短期票据，`DELETE /PlaybackTickets/{ticketId}` 按当前会话撤销；视频/音频 GET/HEAD 在无 Authorization 时接受票据，并保持 Range/ETag/If-Range/206/416 与容器 MIME |
| 多 MediaSource DTO | ✅ 始终可多源；默认排序正式；客户端 UI 仅 observation | ⚠️ 全量可播放 source 列表、本机 URL、七层默认排序及管理员 `PUT /Admin/Items/{itemId}/MediaSources/{mediaSourceId}/PlaybackPolicy` 已实现；deterministic cloud contract 固定两个 MKV source 的完整字段与顺序，按数据库 provider-object 语义归一化动态 ID，并证明显式 default 第一项可播、alternate 仍完整可取。隐藏源同时拒绝 PlaybackInfo、视频、字幕和播放事件，账号健康当前为持久化账号状态代理；真实客户端多版本选择 UI 仍仅作 observation |
| 详情请求 Probe | ❌；只允许 Source Indexing | ✅ 仅 enqueue/join Source Index，不执行 Probe |
| 首次 PlaybackInfo Probe | 有界头/尾 Range，single-flight | ⚠️ MediaSource-scope durable single-flight、精确 storage-root affinity、root-local 祖先/事实来源授权、Filesystem worker 与两端各最多 1 MiB 的 Matroska、ISO-BMFF MP4/M4V 解析，以及 AVC/HEVC profile/level 配置记录提取已实现；provider-neutral 云端 adapter 已覆盖 runtime registry、对象快照、有界 Range 与 Probe commit 门禁 |
| Probe 持久化 | 单 Location 可 Probe；可信 content identity 相同的镜像才可复用；commit 后失效旧缓存 | ⚠️ 单 active Location 快照、首次对象读取前、每次 Range 前、最终对象读取前及 commit 时的重复授权/revision 校验、SQL 原子 commit/generation 已实现；跨镜像可信 content identity 复用与 Redis 失效未实现 |
| Stream index map | MediaSource 范围稳定 delivery index；container index 独立；tombstone 不复用 | ✅ embedded/external 统一 delivery index、独立 container index 与 tombstone 保留已实现 |
| Probe 失败 | 不声明错误 Direct Play | ✅ 确定性解析失败原子写入 ProbeFailed、失败 WorkJob 与 generation；PlaybackInfo 仅声明 Probed source |
| DeviceProfile Direct Play | 精确判断；钉扎 Jellyfin OpenAPI Supports/Protocol/Path/URL golden | ⚠️ 缺省/显式空 profile、container、视频/音频 codec、Width/Height/AudioChannels/VideoProfile/VideoLevel 条件，完整 source 列表与 `SupportsDirectPlay` 条件序列化，以及正式七层默认排序已实现；其余 Jellyfin 条件属性待补齐 |
| Direct Stream / remux / transcode pipeline | ❌ 永久非目标；兼容 flag 不得改变 byte-for-byte 行为 | ❌ |
| Filesystem GET/HEAD/Range | 原文件 byte-for-byte | ✅ secure openat、重启 identity 恢复、GET/HEAD/单 Range 合同测试已实现；真实服务 TCP smoke 使用 literal 请求与完整规范化响应 golden，串联认证、首页/浏览、Lazy Series、详情、PlaybackInfo、完整 GET/HEAD、Range GET/HEAD、外挂字幕、Playing/Progress/Stopped 与 Resume，并在 re-index 后复验交付字节和稳定标识 |
| Audio GET/HEAD/Range | `/Audio/{itemId}/stream` 原文件 byte-for-byte | ✅ 已存在的 Audio CatalogItem 可经浏览筛选与默认 Latest 投影；PlaybackInfo 仅广告本机 `/Audio/{itemId}/stream`，并与鉴权 GET/HEAD/单 Range 使用同一原始字节解析路径。自动音乐库发现、Audio Source Index 与 `MusicAlbum` 层级按 PLAN §5.2 属于后续扩展，不是 v1 门禁 |
| 云盘服务器代理 | `Protocol=Http`，DirectStreamUrl 仅为 TJXY 路由；不重定向、不暴露 URL/token | ⚠️ runtime backend 经本地 TJXY 路由流式代理；provider-neutral deterministic real-TCP contract 已覆盖真实 Source Index/Probe、完整两源响应 golden、默认源完整 GET/HEAD/Range GET/HEAD、alternate 完整 GET、外挂字幕原字节、精确 backend range 序列，以及 PlaybackInfo body 与所有相关 HTTP response header 的 provider/object/drive/account/credential/upstream URL/token 排除。live Google/OneDrive adapter 与 server 的组合验收、生产 tracing 日志脱敏 capture 和多实例协调行为尚未完成 |
| 206 / 416 / If-Range / ETag | ✅ | ✅ 单 Range、suffix/open-ended、If-Range mismatch 与 416 `bytes */size` 已覆盖 |
| 下游断连取消上游 | ✅ | ⚠️ 响应 body 直接持有 backend stream，drop 会取消读取；待真实断连测试 |
| 视频完整缓存 | ❌ | ❌ |
| Range/segment cache | ❌ | ❌ |
| 云端外挂字幕 | OpenAPI 12 路由；TJXY 鉴权、源格式 byte-for-byte，客户端渲染 | ✅ provider-neutral cloud backend 会从 PlaybackInfo 广告的本地 `DeliveryUrl` 经鉴权按源格式 byte-for-byte 拉取；delivery index/URL 在 replacement publication 前后保持稳定 |
| 字幕转换/时间轴重写/burn-in | ❌；不同格式或非零偏移返回 400/415 | ❌ |

---

## 7. Redis 与持久化边界

| 数据 | SQL | Redis | 文件系统 |
|------|-----|-------|----------|
| Catalog / metadata / UserData | SoT；UserData commit 同事务递增 user revision | 可丢弃 DTO/查询缓存 | 否 |
| Storage sync cursor | SoT | 禁止 | 否 |
| OAuth credential | 加密引用/payload | 禁止 | 受控 credential store |
| 图片 | metadata/ref | 可缓存 DTO | 内容寻址 blob |
| Probe 结果 | SoT | PlaybackInfo metadata | 否 |
| 视频字节 | 否 | 禁止 | 原文件或远端对象 |
| Range segment | 否 | 禁止 | 禁止缓存 |

---

## 8. Emby 迁移

| 能力 | 计划 | 状态 |
|------|------|------|
| Emby API Importer | 第一优先级 | ✅ 有界分页、字段归一、opaque checkpoint、加密 API key、可注入 transport、durable worker 与管理员命令面已实现 |
| NFO/本地图 Importer | 第二优先级 | ⚠️ NFO 基础字段 parser 与既有条目 Admin 导入已实现；批量 staging、自动 sidecar、本地图和新条目 identity resolution 未实现 |
| Emby DB Importer | 版本化 best-effort | ⬜ |
| staging + validation + publish | ✅ | ⚠️ replay-safe staging、ReadyToPublish 封存与单事务 Catalog 发布已实现；交互式 Identity Resolution 未实现 |
| dry-run / pause / resume / retry | ✅ | ✅ durable lease/checkpoint/续租、dry-run、pause/resume/reclaim、退避重试及管理员状态/命令面已实现 |
| Legacy ID mapping | 新 ID，不复用 Emby 主键 | ✅ 原子 publish 生成 TJXY Item ID 并写唯一 source-instance 映射 |
| UserData / 图片迁移 | ✅ | ⚠️ UserData、Provider IDs、Genres、People、Studios 已原子发布；远端图片下载与内容哈希去重未实现 |
| 幂等 / 冲突 / 回滚 | ✅ | ⚠️ staging 重放检测、Completed 重放与 publish 全回滚已验证；冲突交互与报告 API 未实现 |

---

## 9. 开放验证项

无。§20 四项均已锁定。

已决策（实现不得回退）：

- Google Drive 初始对象同步 = **Strict Lazy**（见 PLAN §20.1 / ADR-013 / §9.2）。
- 多 MediaSource = **完整多源 DTO + §4.4 正式默认排序**；客户端版本 UI 非 L3 门禁（见 PLAN §20.2 / §4.4）。
- OneDrive = **v1 仅 Personal**；Business/SharePoint 非 v1（见 PLAN §20.3 / §9.3）。
- Metadata provider = **自建 interface + v1 仅 TMDb 远程**；NFO/迁移/命名 fallback；无插件宿主（见 PLAN §20.4 / §11）。
