# API / 功能兼容矩阵

> 计划基线：`PLAN.md` v2.6
> 契约：钉扎 Jellyfin OpenAPI 12.0.0
> 图例：`✅` 计划内 · `⚠️` 最小/待客户端验证 · `⬜` 未实现 · `❌` 非目标

当前实测级别：Phase 0 schema/事务契约、FilesystemBackend 基础、可运行的
L0 发现与 L1 登录/当前用户链路，以及 SQL-only 的 L2 能力上报与基础目录浏览。
L2 当前策略是所有已认证且未禁用的用户可见全部启用媒体库；尚无细粒度媒体库 ACL。
门禁客户端：Findroid；Swiftfin 辅测。

---

## 1. 客户端最小链路

| # | 行为 | 路由 | v2.6 语义 | 状态 |
|---|------|------|-----------|------|
| 1 | 发现 | `GET /System/Info/Public` | 诚实 ProductName/Version | ⚠️ 最小 DTO 已实现，待真实客户端验证 |
| 2 | 登录 | `POST /Users/AuthenticateByName` | canonical MediaBrowser header + aliases | ⚠️ Argon2id 与持久到撤销的 session 已实现，待真实客户端验证 |
| 3 | 当前用户 | `GET /Users/Me` | SQL SoT | ⚠️ token digest 查询已实现，待真实客户端验证 |
| 4 | 能力 | `POST /Sessions/Capabilities/Full` | DeviceProfile 参与 Direct Play 判断 | ⚠️ Full 与 Findroid legacy adapter 已持久化当前 session；尚未接入 PlaybackInfo 判断 |
| 5 | 首页 | `GET /UserViews` | Redis 预热，miss 回源 SQL | ⚠️ SQL SoT 已实现；Redis 预热/ACL 未实现 |
| 6 | 浏览 | `GET /Items` | 未展开 Series 可触发高优先级 Expand | ⚠️ 根视图、父项、类型过滤、稳定分页已实现；递归/Lazy Expand/Redis 未实现 |
| 7 | 主页行 | Latest / Resume / NextUp | SQL + Redis user revision | ⬜ |
| 8 | 详情 | `GET /Items/{id}` | 不触发 Media Probe | ⬜ |
| 9 | 图片 | `GET /Items/{id}/Images/{type}` | 内容寻址 AssetBlob | ⬜ |
| 10 | 播放信息 | `GET|POST /Items/{id}/PlaybackInfo` | 多 MediaSource；首次可惰性 Probe | ⬜ |
| 11 | 原文件 | `GET|HEAD /Videos/{id}/stream` | 本地读取或云盘统一 Range 代理 | ⬜ |
| 12 | 外挂字幕 | `GET /Videos/{id}/{mediaSourceId}/Subtitles/{index}/Stream.{format}` | 鉴权、本机路由、仅源格式 byte-for-byte | ⬜ |
| 13 | 进度 | Sessions Playing/Progress/Stopped/Ping | SQL SoT + Redis 热点刷新 | ⬜ |
| 14 | UserData | UserItems/Favorite/Played | 绑定稳定 CatalogItem ID | ⬜ |

---

## 2. 系统、认证与用户

| 能力 | 计划 | 状态 |
|------|------|------|
| System Info / Public / Ping / Endpoint | ✅ | ⚠️ Public、Ping 与 health 已实现；Endpoint 尚未实现 |
| Branding | ✅ | ⬜ |
| Startup Wizard | ✅ 最小 | ⚠️ 是否已有用户已反映到 Public DTO；交互式向导未实现 |
| AuthenticateByName | ✅ | ⚠️ Username/Pw、统一 401、持久 session 已实现；待客户端验证 |
| canonical `Authorization: MediaBrowser` | ✅ 发布门禁 | ⚠️ Client/Device/DeviceId/Version、Token 与 `ApiKey` query 已实现；待客户端验证 |
| legacy X-Emby/X-MediaBrowser aliases | ✅ | ⚠️ X-Emby-Authorization、X-Emby-Token、X-MediaBrowser-Token、`api_key` 已实现，可配置关闭 |
| Users / Me / Admin CRUD | ✅ | ⚠️ Me 与启动时首管理员已实现；Admin CRUD 未实现 |
| API Keys / Devices / Sessions | ✅ 最小 | ⚠️ 登录 session 与 capabilities 已持久化；API Keys 与管理路由未实现 |
| Quick Connect | ❌ v1 | ❌ |

---

## 3. Library、Storage 与 Tasks

| 能力 | v2.6 计划 | 状态 |
|------|-----------|------|
| VirtualFolders CRUD | Library + StorageRoot + 持久化 effective ScanProfile | ⬜ |
| FilesystemBackend | 正式 | ⚠️ 对象枚举/稳定身份/有界 Range 已有契约测试；Sync 未实现 |
| Google Drive 原生 backend | My Drive + Shared Drive | ⬜ |
| OneDrive 原生 backend | **仅 Personal** | ⬜ |
| OneDrive Business / SharePoint | ❌ v1 非目标；模型可留枚举，绑定 4xx | ❌ |
| rclone/FUSE 强制依赖 | ❌ | ❌ |
| Google Changes | 增量对象同步；removed 才确认缺失 | ⬜ |
| OneDrive Delta | 增量对象同步；deleted 才确认缺失 | ⬜ |
| Storage presence | Present / TemporarilyUnavailable / ConfirmedAbsent | ⬜ |
| Google 初始对象同步 | **Strict Lazy**：标题层 inventory；访问时 scoped sync；禁止默认全树 Inventory First | ⬜ |
| Scoped Storage Sync | Strict Lazy 先物化 SQL 子树，Media Scan 不直连 backend | ⬜ |
| Storage change reconcile | 列级 outbox、dedupe/lease/重放；连续 reconciled sync watermark | ⚠️ lease fencing、退避和连续水位已实现；catalog projector 尚未实现 |
| Filesystem events/additions | 稳定 file ID 配对 move；弱路径只生成 relink 候选 | ⬜ |
| Full / Lazy / Hybrid / Manual | effective policy 写 SQL，Admin/调度器/重启后一致 | ⬜ |
| Storage Tasks | Inventory/Changes/Delta/Auth/Validate | ⬜ |
| Media Tasks | Discover/Resolve/Expand/Index/Probe/Full/Validate | ⬜ |
| ScheduledTasks API | ✅ | ⬜ |

---

## 4. Catalog 与浏览

| 能力 | v2.6 语义 | 状态 |
|------|-----------|------|
| CatalogItem 与路径解耦 | ItemId 不由路径决定 | ⚠️ 领域类型、schema 与基础查询/API 已实现；详情/播放未实现 |
| 跨库 CatalogItem 复用 | `library_catalog_items` 多对多 | ⚠️ 查询以 membership `EXISTS` 校验并防止跨库子项泄漏；写侧尚未实现 |
| 多 MediaSource | 完整多源 DTO + §4.4 正式默认排序；客户端版本 UI 非门禁 | ⬜ |
| MediaSource re-index | 稳定对象/content identity/legacy mapping 保留对外 ID | ⬜ |
| 多 MediaLocation | 一个版本多个镜像 | ⬜ |
| StorageObject 稳定身份 | provider ID/可靠 file ID；Filesystem 路径 fallback 标为 weak | ⬜ |
| Items query/filter/sort/page | 索引 SQL + Redis cache-aside | ⚠️ SQL 类型过滤、`SortName` 升序和 1..=200 分页已实现；递归、其他排序、Redis 未实现 |
| UserViews / Latest / Resume / NextUp | 首页预热 | ⚠️ UserViews SQL 已实现；Latest / Resume / NextUp 与预热未实现 |
| Lazy 初始基础 metadata | title/year/overview/provider/Primary | ⬜ |
| Lazy Movie 首次展开 | 详情触发 Source Index；PlaybackInfo 可等待同一任务；成功 bump generation | ⬜ |
| Lazy Series 首次展开 | publication staging 后一次切换全部 Season/Episode；子 Episode source 已 Indexed | ⬜ |
| Expand single-flight | 先等待 sync/result/reconciled revision，再由多实例 join 持久化 leased job | ⬜ |
| 大型 Series 可见性 | 分批 staging，查询只读取 active publication | ⬜ |
| 跨库子树 membership | 展开子项继承父 Series 当前全部库关联 | ⬜ |
| 空未展开结果缓存 | 短 TTL | ⬜ |
| Search | P1.5 + Redis | ⬜ |
| Manual root scope | 阶段命令必须显式携带 root 或 CatalogItem scope | ⬜ |
| Collections / Playlists | P2 | ⬜ |

---

## 5. Metadata、身份与图片

| 能力 | v2.6 语义 | 状态 |
|------|-----------|------|
| 结构化 metadata | 全部写 SQL | ⬜ |
| NFO / 本地图 | 导入来源，不是运行时 SoT | ⬜ |
| 在线 provider | v1 仅 TMDb，可选；失败不失败整次任务 | ⬜ |
| Provider identity | 可存 TMDb/TVDB/IMDb 等键；在线解析以 TMDb 为主 | ⬜ |
| Jellyfin/Emby metadata 插件宿主 | ❌ | ❌ |
| 弱匹配 | 只生成候选，不强制合并 | ⬜ |
| metadata provenance | 字段来源可追踪 | ⬜ |
| AssetBlob SHA-256 去重 | 相同图片只存一份 | ⬜ |
| ItemAsset 引用 | 图片不归属媒体目录 | ⬜ |
| Admin merge/split/rematch | ✅ | ⬜ |
| Chapter images / Trickplay | ❌，需要视频帧处理 | ❌ |

---

## 6. PlaybackInfo、Probe 与 Stream

| 能力 | v2.6 语义 | 状态 |
|------|-----------|------|
| PlaybackInfo GET + POST | ✅ | ⬜ |
| 多 MediaSource DTO | ✅ 始终可多源；默认排序正式；客户端 UI 仅 observation | ⬜ |
| 详情请求 Probe | ❌；只允许 Source Indexing | ❌ |
| 首次 PlaybackInfo Probe | 有界头/尾 Range，single-flight | ⬜ |
| Probe 持久化 | 单 Location 可 Probe；可信 content identity 相同的镜像才可复用；commit 后失效旧缓存 | ⬜ |
| Stream index map | MediaSource 范围稳定 delivery index；container index 独立；tombstone 不复用 | ⬜ |
| Probe 失败 | 不声明错误 Direct Play | ⬜ |
| DeviceProfile Direct Play | 精确判断；Findroid/Swiftfin 钉扎 Supports/Protocol/Path/URL golden | ⚠️ 服务端 DTO golden 已实现；客户端实测未运行 |
| Direct Stream / remux / transcode pipeline | ❌ 永久非目标；兼容 flag 不得改变 byte-for-byte 行为 | ❌ |
| Filesystem GET/HEAD/Range | 原文件 byte-for-byte | ⬜ |
| 云盘服务器代理 | `Protocol=Http`，DirectStreamUrl 仅为 TJXY 路由；不重定向、不暴露 URL/token | ⬜ |
| 206 / 416 / If-Range / ETag | ✅ | ⬜ |
| 下游断连取消上游 | ✅ | ⬜ |
| 视频完整缓存 | ❌ | ❌ |
| Range/segment cache | ❌ | ❌ |
| 云端外挂字幕 | OpenAPI 12 路由；TJXY 鉴权、源格式 byte-for-byte，客户端渲染 | ⬜ |
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
| Emby API Importer | 第一优先级 | ⬜ |
| NFO/本地图 Importer | 第二优先级 | ⬜ |
| Emby DB Importer | 版本化 best-effort | ⬜ |
| staging + validation + publish | ✅ | ⬜ |
| dry-run / pause / resume / retry | ✅ | ⬜ |
| Legacy ID mapping | 新 ID，不复用 Emby 主键 | ⬜ |
| UserData / 图片迁移 | ✅ | ⬜ |
| 幂等 / 冲突 / 回滚 | ✅ | ⬜ |

---

## 9. 开放验证项

无。§20 四项均已锁定。

已决策（实现不得回退）：

- Google Drive 初始对象同步 = **Strict Lazy**（见 PLAN §20.1 / ADR-013 / §9.2）。
- 多 MediaSource = **完整多源 DTO + §4.4 正式默认排序**；客户端版本 UI 非 L3 门禁（见 PLAN §20.2 / §4.4）。
- OneDrive = **v1 仅 Personal**；Business/SharePoint 非 v1（见 PLAN §20.3 / §9.3）。
- Metadata provider = **自建 interface + v1 仅 TMDb 远程**；NFO/迁移/命名 fallback；无插件宿主（见 PLAN §20.4 / §11）。
