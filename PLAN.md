# TJXY - Jellyfin 兼容媒体目录服务器与原生多云存储网关

> 技术栈：Rust 服务端 + React 管理端
> 协议契约：钉扎的 Jellyfin OpenAPI 12.0.0
> 计划版本：v2.6
> 状态：设计定稿，待实施；§20 四项开放问题均已锁定

---

## 1. 定位、背景与目标

### 1.1 项目定位

TJXY 是一个以数据库为媒体目录核心、支持原生多云存储、逻辑媒体与实际文件位置分离，并兼容 Jellyfin 客户端的媒体服务器。

简写定位：**Jellyfin 兼容媒体目录服务器 + 原生多云存储网关。**

系统不再把“媒体条目”等同于“某个路径下的文件”：

```text
CatalogItem（逻辑作品）
  └── MediaSource × N（具体版本）
        └── MediaLocation × N（本地或云端实际位置）
```

### 1.2 要解决的问题

传统 Emby/Jellyfin 路径中心模式存在以下问题：

- 路径和媒体身份绑定，移动、重命名或迁移存储位置可能影响条目身份和 UserData。
- 同一作品位于多个媒体目录时可能重复创建 metadata 和用户可见条目。
- 同一海报、背景图和 Logo 可能重复保存。
- metadata 文件分散在媒体目录，不利于统一搜索、分类、去重和迁移。
- 大型 Google Drive/OneDrive 媒体库全量扫描会消耗大量 API 请求。
- 大量冷媒体长期不会观看，却在初次建库时被完整展开并读取容器信息。
- 云存储通常依赖 rclone、FUSE 或 OS 挂载，部署和凭据管理复杂。
- 云存储对象同步、媒体识别、metadata 获取和媒体 Probe 被耦合成一个昂贵任务。

TJXY 将这些职责拆开：StorageBackend 负责对象和字节，Storage Sync 负责对象状态，Media Scan 负责理解对象，SQL Catalog 负责结构化目录，Proxy Stream 负责原始字节传输。

### 1.3 目标

| ID | 目标 | 验收标准 |
|----|------|----------|
| G1 | Jellyfin REST 核心契约 | 钉扎 OpenAPI 子集 golden + Findroid 自动化 smoke |
| G2 | Redis 热点缓存 | 首页预热；访问条目 cache-aside；Redis 故障正确回源 SQL |
| G3 | 多数据库 | SQLite、PostgreSQL 同套发布门禁；MySQL 独立实验 smoke |
| G4 | 媒体库与任务 | VirtualFolders、Storage Sync、Media Scan、进度和失败可观测 |
| G5 | React Admin | 向导、用户、存储账号、库、扫描、metadata、迁移和冲突管理 |
| G6 | 功能对位 | `docs/api-parity.md` 与客户端最小链路持续更新 |
| G7 | 原生云存储 | Google Drive、OneDrive 无需 rclone 或 OS 挂载即可建库、扫描和播放 |
| G8 | 多扫描模式 | 每个媒体库可选择 Full、Lazy、Hybrid 或 Manual |
| G9 | 条目与文件解耦 | 一个逻辑作品可以拥有多个媒体版本和多个存储位置 |
| G10 | Metadata 与图片复用 | 相同作品不重复获取 metadata；相同图片 hash 只保存一份 |
| G11 | 云盘增量同步 | Google Drive Changes、OneDrive Delta 正常周期只处理变化对象 |
| G12 | Emby 一键迁移 | metadata、图片、媒体层级和 UserData 可恢复、幂等迁移 |
| G13 | 统一代理播放 | 所有云盘媒体通过服务器 Range 代理，不泄露上游链接或凭据 |
| G14 | 无视频缓存 | 播放不缓存完整文件、Range 分块或离线副本 |

### 1.4 兼容级别

| 级别 | 含义 | 客户端表现 |
|------|------|------------|
| L0 发现 | SystemInfo + Branding | 能发现服务器 |
| L1 登录 | Auth + Users/Me + Session token | 能登录 |
| L2 浏览 | UserViews + Items + Images | 能浏览目录和详情 |
| L3 播放 | PlaybackInfo + Stream + UserData | Direct Play 和进度同步 |
| L4 运维 | VirtualFolders + Tasks + Admin Users | 能建库、同步和扫描 |
| L5 增强 | WS / Search / Collections | 实时刷新和高级组织 |

v1 发布目标仍为 L0-L4；云存储能力按 §17 的阶段逐步进入发布门禁。

### 1.5 不可变硬边界

- 服务端永久只做 Direct Play 原始字节传输。
- 不实现媒体解码、编码、转码、remux、Direct Stream、HLS、DASH、字幕转换、烧录字幕或 Trickplay。
- Google Drive、OneDrive 使用原生 API，不要求 rclone、FUSE 或 OS 挂载。
- SMB/NFS 仍可由 OS 挂载后通过 FilesystemBackend 使用。
- 不向客户端返回 Google/Microsoft 临时下载 URL、OAuth token 或 refresh token。
- 所有云盘视频、音频和外挂字幕统一经过本服务器鉴权流式代理；图片先导入内容寻址 asset store 后由本机提供。
- 不缓存完整视频、不缓存 Range segment、不预取视频、不提供离线视频缓存。
- 允许缓存图片、图片派生尺寸、结构化 metadata、Probe 结果和 Redis DTO。
- 代理中的小型有界网络缓冲区只用于背压，不是可复用缓存。
- StorageBackend 只提供对象与字节能力，不得演变为转码或媒体处理系统。
- 在线 metadata provider 失败不得导致整次 Storage Sync 或 Media Scan 失败。

---

## 2. 架构决策

### ADR-001：Jellyfin-first 协议

Jellyfin OpenAPI 是唯一主要协议契约。`ProductName` 和 `Version` 诚实报告；Emby 私有路由和 `/emby/*` 不作为 v1 主契约。

### ADR-002：SQL SoT + Redis cache-aside

- SQL 是所有结构化 metadata、Catalog、StorageObject、扫描游标、Probe 结果和 UserData 的权威源。
- Redis 只缓存首页、查询结果、访问过的 DTO 和 PlaybackInfo metadata。
- Redis miss 回源 SQL；Redis 丢失不影响正确性。
- 冷条目首次访问允许 SQL，热点命中时 0-SQL。

### ADR-003：SQLite/PG 正式支持，MySQL 实验支持

使用 SeaORM + SeaQuery。SQLite、PostgreSQL 跑同一套迁移、契约和集成测试；MySQL 仅 nightly best-effort。

### ADR-004：React Admin

自建 React + TypeScript 管理端。第三方 Jellyfin 客户端承担主要播放体验。

### ADR-005：分层扫描与多扫描模式

| 项 | 决策 |
|----|------|
| 选择 | Media Scan 拆分为 Storage Object Selection、Title Classification、Metadata Resolution、Structure Expansion、Media Source Discovery、Media Probe |
| 模式 | 每个媒体库支持 Full、Lazy、Hybrid、Manual |
| 原因 | 大型云端冷媒体库不应在初次建库时读取全部内部目录和媒体头 |
| 后果 | 每个阶段有独立状态、任务、重试和优先级；用户访问可触发高优先级展开 |

Filesystem 仍可使用 additions-only 快速扫描，但该假设不适用于所有 StorageBackend。

### ADR-006：Direct Play only

TJXY 的能力事实固定为 byte-for-byte 原文件传输：不 remux、不转码、不改容器，`SupportsTranscoding=false` 且 `TranscodingUrl=null`。协议基线是在客户端 DeviceProfile 支持源容器和编码时声明 `SupportsDirectPlay=true`、`SupportsDirectStream=false`；但 `Protocol`、`Path`、`DirectStreamUrl`、`IsRemote`、`SupportsDirectPlay`、`SupportsDirectStream` 和 `static=true` 的序列化组合必须通过固定版本 Findroid/Swiftfin fixture 实测后钉扎 golden。若某客户端只有在 `SupportsDirectStream=true` 时才接受 TJXY 的原字节 HTTP 路由，可以仅把该字段作为兼容方言调整，但实际响应仍必须 byte-for-byte、不得出现 remux/transcode pipeline，并须新增 ADR 和回归门禁。

### ADR-007：ScheduledTasks 对齐

保留 Jellyfin ScheduledTasks 路由和任务状态机，但内部拆分 Storage Tasks 与 Media Tasks，见 §14。

### ADR-008：MediaBrowser 序列化方言

默认 JSON PascalCase；GUID 为小写带连字符字符串；时长使用 100ns ticks；日期按钉扎契约输出。

### ADR-009：SQL generation + Redis revision keys

SQL `catalog_state.generation` 是目录世代。Redis key 携带 catalog generation 和 user revision；SQL 提交是发布边界，缓存预热失败只进入 degraded 状态，旧 key 由 TTL 淘汰。

### ADR-010：CatalogItem tombstone

StorageObject 只有进入 ConfirmedAbsent 后才能触发 MediaLocation 缺失对账；TemporarilyUnavailable 不得删除 Location 或 CatalogItem。CatalogItem 在最后可用 Location 消失后进入 tombstone/detach，UserData 绑定稳定 CatalogItem ID；只有显式 purge 才硬删。

### ADR-011：逻辑条目与存储位置分离

| 模型 | 职责 |
|------|------|
| CatalogItem | Movie、Series、Season、Episode 等逻辑作品及其 metadata |
| MediaSource | 同一作品的具体版本，例如 4K、1080p、导演剪辑版 |
| MediaLocation | 某个 MediaSource 的本地或云端实际位置/镜像 |

ItemId 不由路径决定；UserData 绑定 CatalogItem；MediaStreams 绑定 MediaSource。

### ADR-012：可插拔原生 StorageBackend

首批 adapter：Filesystem、Google Drive、OneDrive。Scanner、Storage Sync 和 Proxy Stream 只依赖统一 interface，不直接依赖云服务 SDK。

概念 interface：

```rust
trait StorageBackend {
    async fn get_object(&self, id: &StorageObjectId) -> Result<StorageObject>;
    async fn list_children(&self, parent: &StorageObjectId, page: PageToken) -> Result<ObjectPage>;
    async fn list_changes(&self, cursor: ChangeCursor) -> Result<ChangePage>;
    async fn open_range(&self, id: &StorageObjectId, range: ByteRange) -> Result<RangeStream>;
    fn capabilities(&self) -> StorageCapabilities;
}
```

`list_changes` 对不支持原生增量的 backend 返回明确的 capability error；业务层根据 capabilities 选择策略。

### ADR-013：Storage Sync 与 Media Scan 分离

- Storage Sync 同步对象 ID、名称、父子关系、size、checksum、revision、移动和删除。
- Media Scan 只从 SQL 中选择已同步的 StorageObject，再分类为 CatalogItem、MediaSource、MediaLocation、Subtitle 等领域对象；不枚举 backend、不推进 sync cursor。
- 任何 Structure Expansion 或 Source Indexing 若发现目标 scope 尚未物化，必须先调度并等待 scoped/on-demand Storage Sync；Media 任务等待 SQL 可见的 sync revision 后再继续，绝不直接调用 `list_children`。
- Google Drive 初始对象同步锁定 **Strict Lazy**：初始只物化标题层 StorageObject；更深子树仅在访问/Expand/Index 时按 scope 物化。禁止默认全树 Inventory First。
- Inventory First 仅可作为管理员显式 Full Validate/Inventory 命令的实现路径，不得作为 Google root 绑定后的默认初始策略。
- Strict Lazy 与全树 inventory/validate 共享同一 Storage Sync -> SQL -> Media Scan 流水线，只是预取范围和时机不同；两者使用独立队列、状态、优先级和进度。

### ADR-014：内容寻址图片存储

图片按 SHA-256 内容地址保存。CatalogItem 通过引用关联图片；相同内容只保存一份。删除条目不立即删除 blob，由引用检查和 GC 清理。

### ADR-015：云盘统一服务器代理

所有云盘视频、音频和外挂字幕通过 TJXY 的鉴权路由代理。禁止 302/307 到云服务，禁止在 DTO 中暴露上游 URL。视频/音频代理负责 GET/HEAD/单 Range、背压、限流、超时、OAuth 刷新和下游断连取消；字幕通过 TJXY Subtitle Stream 路由 byte-for-byte 返回。

### ADR-016：无视频缓存

不缓存完整视频或 Range segment。重复播放和 seek 会重新请求上游 Range。允许持久化 Probe 结果和缓存图片/metadata。

### ADR-017：可恢复 Emby 迁移管线

迁移流程固定为：

```text
Adapter -> Staging -> Identity Resolution -> Validation -> Publish
```

支持 Emby API、NFO/本地图、Emby DB best-effort。迁移可 dry-run、暂停、恢复、重试、幂等和回滚，并保存 Legacy ID 映射。

---

## 3. 产品决策

| ID | 决策 | 落地约束 |
|----|------|----------|
| PD-001 | 诚实 ProductName | 发布构建不默认伪装 Jellyfin/Emby |
| PD-002 | Findroid 主门禁，Swiftfin 辅测 | 固定客户端版本和自动化链路 |
| PD-003 | 媒体消失后 detach | tombstone 保留 ItemId 和 UserData，显式 purge 才硬删 |
| PD-004 | MySQL best-effort | 不作为发布门禁，不宣称生产支持 |
| PD-005 | 本地 + 原生云存储 | Filesystem、Google Drive、OneDrive Personal 正式；Business/SharePoint 非 v1；SMB/NFS 可继续 OS 挂载 |
| PD-006 | 永久 Direct Play only | 无转码、remux、分片或媒体处理扩展点 |
| PD-007 | 离线 metadata 可导入，远程仅 TMDb 可选 | NFO/本地图可导入 SQL；v1 唯一远程 provider 为 TMDb；远程失败不失败整次任务；无 Jellyfin 插件宿主 |
| PD-008 | Redis cache-aside | auto 仅探测配置的本机地址；故障回源 SQL |
| PD-009 | 按 backend 能力增量同步 | Filesystem events/additions；Google Changes；OneDrive Delta |
| PD-010 | Lazy 初始基础 metadata | 标题、年份、简介、Provider ID、Primary 海报；不展开、不 Probe |
| PD-011 | Series 首次进入完整展开 | single-flight、临时结果、完整校验、原子发布全部 Season/Episode |
| PD-012 | 首次 PlaybackInfo Probe | 按 MediaSource single-flight；只读必要头/尾 Range；结果写 SQL |
| PD-013 | 无视频缓存代理 | 云盘字节统一代理，不缓存完整视频或 Range 分块 |

### 3.1 凭据规则

- OAuth token 加密保存，普通配置文件只允许保存 credential reference。
- token、Authorization header 和上游临时 URL 不写日志、不进入 Redis、不返回客户端。
- 支持 token 自动刷新、重新授权和密钥轮换。
- 一次认证/API 失败不得直接删除 CatalogItem；对象标记为 unavailable 并重试。

### 3.2 Backend 增量策略

| Backend | 正常增量 | 一致性维护 |
|---------|----------|------------|
| Filesystem | FS event + additions scan | 显式 Validate/Full Scan |
| Google Drive | Changes API | token 失效时按 provider 恢复协议重新 inventory |
| OneDrive | Delta API | deltaLink 失效时按 provider 恢复协议重新 inventory |

云端增量原生处理新增、修改、移动、重命名和删除。StorageObject 的暂时不可访问与已确认删除必须区分。

---

## 4. 协议与客户端行为

### 4.1 最小成功链路

```text
1.  GET  /System/Info/Public
2.  POST /Users/AuthenticateByName
3.  GET  /Users/Me
4.  POST /Sessions/Capabilities/Full
5.  GET  /UserViews?userId=...
6.  GET  /Items?userId=...&parentId=...
7.  GET  /Items/Latest | /UserItems/Resume | /Shows/NextUp
8.  GET  /Items/{itemId}?userId=...&fields=...
9.  GET  /Items/{itemId}/Images/{type}
10. GET|POST /Items/{itemId}/PlaybackInfo
11. GET|HEAD /Videos/{itemId}/stream?static=true&mediaSourceId=...
12. GET /Videos/{itemId}/{mediaSourceId}/Subtitles/{index}/Stream.{format}
13. POST /Sessions/Playing | Progress | Stopped | Ping
14. GET|POST /UserItems/{itemId}/UserData
```

### 4.2 控制器范围

| 区域 | v1 行为 |
|------|---------|
| System / Branding / Startup | 标准 Jellyfin 发现和首次启动子集 |
| Auth / Users / API Keys | canonical MediaBrowser header + legacy aliases |
| LibraryStructure | VirtualFolders 映射到 library + storage roots + scan profile |
| ScheduledTasks | Storage Tasks 与 Media Tasks |
| Items / UserViews / Home rows | SQL + Redis cache-aside；支持 Lazy 展开触发 |
| Images | 内容寻址 asset store |
| PlaybackInfo | 多 MediaSource；必要时惰性 Probe |
| Videos / Audio | 本地读取或云端统一 Range 代理 |
| UserData / Playstate | SQL SoT + Redis user revision |
| Sessions / DisplayPreferences | v1 最小兼容 |
| Search | P1.5，可缓存结果 |
| WebSocket | P2 LibraryChanged/UserDataChanged |

### 4.3 Lazy 浏览触发

请求未展开 Series 的子项：

```text
GET /Items?ParentId={seriesId}
  -> single-flight Expand Item
  -> wait_with_timeout
```

- 建议初始等待 2-3 秒，具体值可配置。
- 时间内完成则返回完整 Season/Episode。
- 超时返回当前已发布结果，不暴露临时半成品。
- 完成后递增 catalog generation；P2 发送 LibraryChanged。
- 未展开导致的空结果使用短 TTL，不能长时间缓存。

Movie 的 Source Indexing 由首次 `GET /Items/{movieId}` 详情触发，使用与 Series 相同的 wait timeout 和原子发布基础设施，但只发布该 Movie 的 MediaSource、MediaLocation 和字幕，不执行 Probe。若客户端跳过详情直接请求 PlaybackInfo，PlaybackInfo 必须先完成或等待同一个 Source Indexing，再进入 Probe；超时或索引失败时返回无可播放源，而不是虚构 Direct Play。

### 4.4 PlaybackInfo 与多源

首次请求检查每个候选 MediaSource 的 Probe 状态。需要时执行高优先级 single-flight Probe；失败的 source 不得错误声明 Direct Play。

所有对客户端交付的 MediaSource 使用 TJXY HTTP 路由。基线 DTO：`Protocol=Http`；`DirectStreamUrl` 为带 `static=true&mediaSourceId={stableId}` 的 TJXY `/Videos/.../stream` 或 `/Audio/.../stream`；`Path` 为空或为不含 backend 信息的安全展示值；`TranscodingUrl=null`；三个 Supports flag 按 ADR-006。DTO、响应 header 和日志不得出现本地真实路径、Google/Graph 下载域名、上游临时 URL 或 token。最终 flag 方言以 §18.10 固定客户端 golden 为准，但安全与 byte-for-byte 边界不可调整。

PlaybackInfo **必须可返回多个 MediaSource**（禁止为迁就弱客户端而只返回单源）。客户端不提供版本 UI 或未传 `MediaSourceId` 时，服务器按以下**正式默认排序**选择默认源（列表仍完整返回）：

1. DeviceProfile 可以 Direct Play；
2. 用户上次使用的 source；
3. 存储位置可用；
4. 管理员优先级 / 显式默认源；
5. 分辨率；
6. 编码兼容性；
7. 存储账号健康状态。

**v1 门禁**：Findroid L3 只要求默认源真播成功；客户端多版本选择 UI **不是**发布门禁。Swiftfin/Infuse 若有版本 UI，仅作 observation 记录（能否切换、是否传对 `MediaSourceId`），失败不阻塞发布。Admin 可设置默认 MediaSource、优先级与隐藏不可用源。

---

## 5. 领域模型

### 5.1 核心关系

```text
Library
  └── LibraryCatalogItem -> CatalogItem
                              ├── ProviderIdentity × N
                              ├── ItemAsset × N -> AssetBlob
                              ├── UserData × N
                              └── MediaSource × N
                                    ├── MediaStream × N
                                    └── MediaLocation × N
                                          └── StorageObject
                                                └── StorageAccount/Root
```

`LibraryCatalogItem` 是显式多对多关联，使同一 CatalogItem 可在多个媒体库出现而不复制 metadata。

### 5.2 CatalogItem

```text
catalog_items
- id
- type
- parent_id
- name
- original_title
- sort_name
- production_year
- overview
- classification_state
- metadata_state
- structure_state
- source_state
- structure_expansion_revision
- source_index_revision
- active_structure_publication_id
- active_source_publication_id
- is_present
- created_at
- updated_at
- last_expanded_at
- last_error
```

类型包括 Movie、Series、Season、Episode、Folder；MusicAlbum 等后续扩展。UserData 始终绑定 `catalog_item_id`。

### 5.3 MediaSource

```text
media_sources
- id
- catalog_item_id
- presentation_key
- edition
- container
- video_codec
- resolution
- bitrate
- runtime_ticks
- probe_state
- probe_revision
- probe_location_id
- probe_location_revision
- probe_content_identity
- last_probe_error
```

MediaSource 表示具体版本，不等于路径。`id` 是数据库主键；`presentation_key` 是对外 Jellyfin `MediaSourceId`，首次发布时生成不可变 UUID，并对 `(catalog_item_id, presentation_key)` 唯一。re-index 按以下证据复用原 key：已关联的稳定 StorageObject ID、可信 ContentIdentity、legacy mapping 或管理员确认；路径/标题弱匹配只能生成候选。相同逻辑版本的对象 revision/位置变化保留 key；确认是不同版本时创建新 key，并以 tombstone/replacement mapping 保留旧引用。媒体容器和流字段在未 Probe 时允许为空。

### 5.4 MediaLocation

```text
media_locations
- id
- media_source_id
- storage_object_id
- content_identity
- content_identity_kind
- priority
- availability_state
- last_success_at
- last_error
```

一个 MediaSource 可以有多个镜像位置。任一单独 Location 都允许被 Probe，无需先拥有跨位置 ContentIdentity；Probe 结果同时记录实际 `probe_location_id` 和该对象 revision。结果始终可用于该 Location，只有 backend 可信 checksum、已验证内容身份或管理员确认一致时，其他 Location 才能共享同一 MediaSource 和 Probe 结果。`storage_object_id` 唯一指向实际对象；路径只用于显示或 Filesystem identity。无法证明内容一致的对象必须建立不同 MediaSource，不能把一个 Location 的 Probe 结果套用到另一个对象。

### 5.5 MediaStreams 与字幕

`media_streams.media_source_id` 关联 MediaSource，不能只关联 CatalogItem，因为不同版本具有不同音视频轨道。每个 Source 的 stream `index` 在未变更轨道集合时保持稳定。

外挂字幕同时记录为 `subtitles` 和 PlaybackInfo `MediaStreams`：`Type=Subtitle`、稳定 `Index`、源 `Codec/Language`、`IsExternal=true`、`IsTextSubtitleStream`、`IsDefault`、`IsForced`、`DeliveryMethod=External`，`DeliveryUrl` 只指向 TJXY 字幕路由。内封字幕保留在 MediaStreams 供 Direct Play 客户端从容器读取，不提供提取、转换或 burn-in。

`media_stream_index_map(media_source_id, stream_identity, delivery_index, container_stream_index, stream_type, is_present)` 对 `(media_source_id, delivery_index)` 和 `(media_source_id, stream_identity)` 唯一。首次 Probe 后统一分配：内封轨优先沿用容器 stream index；外挂字幕按稳定 StorageObject/subtitle identity 分配下一个未使用 index。已有映射优先且永不复用已 tombstone index；若后续新增内封轨与现有 delivery index 冲突，新轨取得下一个空闲 index，同时保留真实 `container_stream_index`。PlaybackInfo 和字幕 URL 都使用 delivery index；底层容器选择使用单独的 container index。Source Indexing 可登记外挂字幕 identity，但必须等首次 Probe 完成统一分配后才发布完整 PlaybackInfo。

### 5.6 身份匹配与去重

新增：

```text
provider_ids
identity_matches
metadata_provenance
library_catalog_items
```

匹配优先级：

1. TMDb/TVDB/IMDb 等 Provider ID；
2. NFO 唯一 ID；
3. Emby Legacy Item ID 映射；
4. Series ID + Season Number + Episode Number；
5. 标准化标题 + 年份；
6. 文件夹命名规则。

标题和年份只能生成候选。低置信度不得自动强制合并。Admin 必须支持未匹配项、重复候选、手动合并/拆分、Provider ID 修正和重新匹配。

相同媒体文件的自动复用仅在 backend 提供可信 checksum/内容身份或管理员确认时进行；不得为了去重而默认完整读取视频计算 hash。

### 5.7 状态机

CatalogItem 使用正交状态字段：

```text
classification_state = Unclassified | Classifying | Matched | Unmatched | Failed
metadata_state       = Empty | Resolving | Partial | Ready | Failed
structure_state      = NotApplicable | Unexpanded | Expanding | Expanded | Failed
source_state         = Unknown | Indexing | Indexed | Failed
last_expanded_at
last_error
```

Admin 可从这些字段派生 Discovered、TitleMatched、MetadataReady、StructureExpanded、SourcesIndexed、Playable、Failed 等阶段标签，但数据库不保存单一线性总状态。`Playable` 仅在至少一个 MediaSource 已 Probed 且存在可用 Location 时成立；Lazy 条目在首次 Probe 前可浏览但不宣称可播放。

MediaSource Probe 状态：

```text
NotProbed | Probing | Probed | ProbeFailed | Stale
```

---

## 6. 存储模型

### 6.1 核心表

```text
catalog_state
users
user_catalog_state
libraries
catalog_items
library_catalog_items
media_sources
media_source_aliases
media_locations
media_streams
media_stream_index_map
subtitles
provider_ids
identity_matches
metadata_provenance
people / item_people
genres / item_genres
studios / item_studios
user_data
storage_accounts
storage_credentials
storage_roots
storage_objects
storage_sync_cursors
storage_change_outbox
library_storage_roots
asset_blobs
item_assets
work_jobs
work_staging_rows
work_results
```

- `catalog_state` 保存 catalog generation；`user_catalog_state` 保存每用户单调 revision，`user_data` 保存条目级持久状态。
- `media_source_aliases(alias_key, media_source_id, reason, created_at)` 保存 re-index/迁移时旧 presentation key 到当前 Source 的映射；`media_stream_index_map` 固定对外 stream index；`subtitles` 关联 MediaSource 和对应 StorageObject，记录源格式、语言、稳定索引和默认/强制标志。
- People/Genres/Studios 及关联表支撑搜索、DTO 和 Emby 迁移，不由 Redis 代替。
- `storage_credentials` 只保存加密 payload、key version 和刷新状态；应用日志、Redis 和普通配置不得包含明文 token。

### 6.2 Library 与 UserData revision

```text
libraries
- id
- name
- scan_profile
- object_selection_scope
- metadata_policy
- expansion_policy
- probe_policy
- profile_version
- created_at
- updated_at

user_catalog_state
- user_id
- revision
- updated_at

user_data
- user_id
- catalog_item_id
- playback_position_ticks
- is_played
- play_count
- is_favorite
- last_played_at
- updated_at
```

`scan_profile` 固定为 `Full | Lazy | Hybrid | Manual`。选择预设时，同一事务把 §10.1 对应的四个 effective policy 写入 Library；高级设置修改后保留 profile 名和 `profile_version`，但调度器只读取 SQL 中的 effective policy，不依赖代码隐藏默认值。VirtualFolders DTO 和 Admin 表单都映射同一行。

每次提交 UserData 变更时，在同一事务 upsert `user_data` 并递增对应 `user_catalog_state.revision`。播放进度可以按配置节流，但每个实际 SQL commit 只递增一次；Favorite、Played、PlayCount 和 Position 使用同一 revision 空间。Redis user-scoped key 固定包含 `g:{catalog_generation}:u:{user_id}:r:{user_revision}`，因此 Redis 失败或并发更新不会使已提交 UserData 被旧 Resume/NextUp/Home key 遮蔽。

### 6.3 StorageAccount

```text
- id
- provider
- display_name
- account_identity
- credential_ref
- status
- last_authenticated_at
```

`storage_roots` 另保存 `sync_revision` 和 `reconciled_sync_revision` 两个单调 bigint。前者标识已提交 StorageObject 批次，后者标识对应 outbox 已按顺序完成 catalog 对账的连续水位。

Provider：`filesystem`、`google_drive`、`onedrive_personal` 为 v1 正式实现。`onedrive_business`、`sharepoint` **仅保留模型枚举**，v1 不得开放绑定/同步/播放；创建或绑定时返回明确 4xx（如 `provider_not_in_v1`）。

### 6.4 StorageObject

```text
- id
- storage_account_id
- provider_object_id
- provider_drive_id
- provider_parent_id
- name
- normalized_name
- object_type
- mime_type
- size
- checksum
- etag
- remote_revision
- remote_modified_at
- observed_sync_revision
- children_indexed
- children_index_revision
- identity_quality
- presence_state
- availability_reason
- last_listed_at
```

唯一身份：

- Filesystem：优先 backend instance + 稳定 file ID/inode（仅在平台声明可靠时）；否则退化为 backend instance + canonical path，并标记 `identity_quality=path_weak`；
- Google Drive：account + file ID；
- OneDrive：account + drive ID + item ID。

云盘路径不是身份。移动和重命名更新对象关系，不创建新的 CatalogItem。Filesystem event adapter 在同一 quiet window 内优先用稳定 file ID 配对 rename/move；无可靠 ID 时，新旧路径只通过 size、mtime、命名和可选 checksum 生成候选，不得自动认定同一对象。未确认候选保留原 CatalogItem/UserData tombstone，并进入 Admin relink/merge 队列；Validate 不得因弱路径身份直接 purge UserData。

`presence_state` 固定为 `Present | TemporarilyUnavailable | ConfirmedAbsent`：成功 inventory/change/get 将对象置为 Present；认证失败、429、超时和 5xx 只置 TemporarilyUnavailable；Google Changes removed、OneDrive Delta deleted，或一次完整且成功的 backend validate 确认缺失后，才置 ConfirmedAbsent。普通对象读取的单次 404 不足以确认删除，必须经增量事件或 validate 复核。MediaLocation availability 从该状态派生，不能反向充当 Storage Sync SoT。

### 6.5 StorageSyncCursor

```text
- storage_root_id
- cursor_type
- cursor_value
- last_success_at
- last_full_sync_at
- status
```

Google page token、OneDrive deltaLink 等作为 opaque string 存入 SQL，业务层不得解析，也不得放入 Redis。

`storage_change_outbox` 列级契约：

```text
- id
- storage_root_id
- sync_revision
- event_type
- storage_object_id
- before_object_revision
- after_object_revision
- payload_version
- payload
- dedupe_key
- state
- attempt_count
- lease_owner
- lease_expires_at
- available_at
- created_at
- processed_at
- last_error
```

`dedupe_key` 唯一，固定由 `(storage_root_id, sync_revision, storage_object_id, event_type)` 构成；payload 只含版本化对象差异和关系 ID，不含 credential、下载 URL 或视频字节。consumer 以 compare-and-swap 获取 lease，采用 at-least-once delivery；catalog 变更、outbox `Processed` 和 root `reconciled_sync_revision` 连续水位在同一事务提交。失败递增 attempt 并按 backoff 更新 available_at；越过缺口的事件可以预处理但不能推进连续水位。

### 6.6 图片资产

```text
asset_blobs
- id
- sha256
- mime_type
- width
- height
- byte_size
- local_relative_path
- created_at

item_assets
- item_id
- asset_blob_id
- image_type
- priority
- source_provider
- source_reference
```

流程：获取图片 -> 限制大小/像素 -> 计算 SHA-256 -> 复用或原子写入 asset store -> 创建引用。GC 只删除无引用且超过保留期的 blob。

### 6.7 持久化工作任务与发布集

```text
work_jobs
- id
- task_kind
- scope_type
- scope_id
- expected_revision
- required_sync_job_id
- input_sync_revision
- state
- priority
- attempt_count
- lease_owner
- lease_expires_at
- created_at
- started_at
- completed_at
- last_error

work_staging_rows
- job_id
- publication_id
- entity_kind
- natural_key
- payload
- validation_state

work_results
- job_id
- counters
- warnings
- error_summary
```

`work_jobs` 对 active 状态建立 `(scope_id, task_kind, expected_revision)` 唯一约束。第一个请求创建任务，其他请求 join 同一 job 并等待通知/轮询；API wait timeout 只返回当前 active publication，绝不读取 staging。worker 通过 compare-and-swap 获取有期限 lease，崩溃后由其他 worker 接管；staging 按 natural key 幂等续写。

Structure Expansion 和 Source Indexing 可以把超大结果分批写入带 `publication_id` 的 staging/shadow rows。完整性校验通过后，最终短事务必须再次比较 expected revision，然后切换 CatalogItem 的 active publication pointer、更新状态、递增 catalog generation、写 cache invalidation 并标记 job completed。revision 已变化时整批结果作废并重排任务；旧 publication 延迟 GC。对外仍只观察到旧全集或新全集，不观察批次中间态。

---

## 7. 系统架构与模块

```text
Google Drive / OneDrive / Filesystem
                  |
                  v
            StorageBackend
                  |
          +-------+--------+
          v                v
     Storage Sync      Proxy Stream
          |                |
          v                |
    Storage Objects        |
          |                |
          v                |
 Full/Lazy/Hybrid/Manual   |
          Media Scan       |
          |                |
          v                |
      SQL Catalog          |
   CatalogItem             |
      |                    |
      +-- MediaSource      |
      |      +-- MediaLocation
      +-- Metadata         |
      +-- Asset References |
      +-- UserData         |
          |                |
          v                |
      Jellyfin API         |
          +-- Items        |
          +-- Search       |
          +-- PlaybackInfo-+
          +-- Stream
                  |
                  v
               Client
```

### 7.1 Repository 结构

```text
crates/
├── server/
├── api/
├── application/
├── domain/
├── db/
├── cache/                    # Redis cache-aside
├── storage/                  # interface、对象、Range、cursor、registry
├── storage-filesystem/
├── storage-google-drive/     # OAuth、Files、Changes、Shared Drive、stream
├── storage-onedrive/         # OAuth、drive items、Delta、stream
├── storage-sync/             # initial、incremental、reconciler、scheduler
├── scanner/                  # 分类、展开、source discovery、Probe 调度
├── metadata/                 # provider interface、provenance、identity resolution
├── assets/                   # 内容寻址图片与 GC
├── media/                    # naming、MediaSource、DeviceProfile
├── proxy-stream/             # GET/HEAD/Range、背压、取消、限流
├── import/                   # Emby adapters、staging、publish
├── tasks/
└── common/
```

依赖规则：

- `scanner`、`storage-sync`、`proxy-stream` 只依赖 `storage` interface。
- Google/OneDrive SDK 只出现在各自 adapter crate。
- API 不依赖云服务 SDK，也不读取 token。
- OAuth 和 token 刷新封装在具体 storage adapter。
- `storage` 不依赖 scanner、media、API 或 proxy-stream。
- `media` 负责命名解析、MediaSource 和 DeviceProfile，不负责网络存储。

---

## 8. Redis 热点缓存

### 8.1 缓存范围

允许缓存：UserViews、Latest、Resume、NextUp、Items 查询结果、搜索结果、最近访问 Item DTO、PlaybackInfo metadata。

明确禁止缓存：OAuth/refresh token、云盘下载 URL、视频字节、Range segment、必须持久化的扫描游标和迁移 staging。

### 8.2 自动探测与降级

```toml
[redis]
mode = "auto" # auto | enabled | disabled
url = "redis://127.0.0.1:6379"
connect_timeout_ms = 200
key_prefix = "tjxy"
home_ttl_seconds = 300
item_ttl_seconds = 1800
empty_expansion_ttl_seconds = 3
```

- auto 只探测配置的本机地址；PING 成功启用，失败无缓存运行。
- enabled 连接失败使 readiness 失败；disabled 不连接。
- 运行时错误视为 miss 并回源 SQL，配合超时和熔断。
- user-scoped Items/Home/Resume/NextUp key 使用 `tjxy:v1:g:{catalog_generation}:u:{user_id}:r:{user_revision}:{projection}:{query_hash}`；非用户 DTO 省略 user 段；PlaybackInfo key另携带 MediaSource probe revision digest。
- Structure Expansion、Source Indexing 和 Probe 成功提交都递增 catalog generation，并主动删除受影响 Item/PlaybackInfo key；SQL commit 是写成功边界，Redis 刷新失败只标 degraded。
- 并发 miss 使用 bounded single-flight，不能让缓存锁阻断 SQL 回源。

---

## 9. Storage Sync

### 9.1 职责

Storage Sync 只维护 StorageObject 事实，不执行标题匹配、metadata provider、Series 展开或媒体 Probe。

Storage Sync 支持 root scope 和 subtree scope。每次成功提交对象页/批次时，在同一事务递增 `storage_roots.sync_revision`，写入该 revision 的对象和 outbox；scoped sync 仅在最后一页完成时将目标 parent 的 `children_indexed=true`、`children_index_revision=sync_revision`，并把该值写入 sync job result。

Structure Expansion/Source Indexing 遇到 `children_indexed=false` 时创建高优先级 `Scoped Storage Sync` 并保存 `required_sync_job_id`。Media job 只有在 sync job Completed、目标 parent 的 `children_index_revision >= result_sync_revision` 且 root `reconciled_sync_revision >= result_sync_revision` 后，才能写入自己的 `input_sync_revision`、捕获当前 item expected revision 并开始。这样“SQL 可见且已完成 catalog 对账”是唯一等待条件。scope 由 StorageObject ID 表达，不把路径或云端 URL 交给 Media Scan。

### 9.2 Google Drive

支持 OAuth、My Drive、Shared Drive、多账号、远端目录选择、Changes API、token 自动刷新、限流重试和 Range。

```text
首次：绑定账号 -> 选择 Drive/root -> Strict Lazy 标题层 inventory -> 保存 page token
访问/Expand：目标子树未物化则 scoped Storage Sync -> SQL 可见且 reconciled 后 Media Scan
后续：读取 Changes -> 幂等更新已物化对象与相关祖先 -> 提交新 page token
```

**Strict Lazy（已锁定）**：
- 初始 inventory 只同步 library root 的标题层（root 直接子项及标题发现所需最小对象集），不递归全树。
- 首次进入 Movie/Series 或 Source Indexing 发现 `children_indexed=false` 时，触发一次高优先级 scoped Storage Sync；同一 scope 必须 single-flight。
- Media Scan 在 sync job Completed 且 `children_index_revision`、`reconciled_sync_revision` 覆盖 result revision 之前不得启动。
- Changes API 仍以 page token 为增量游标；对尚未物化的子树，变更可记录 cursor 进展，但不得迫使系统提前全树物化。token 失效时按 provider 恢复协议重建标题层基线，再按访问 scope 补齐，不默认切换为全树 Inventory First。
- 管理员显式 Full Inventory/Validate 可递归物化整个 root，但这是运维命令，不是绑定后的默认路径。

### 9.3 OneDrive

优先支持 OneDrive Personal、Microsoft OAuth、Delta API、Range 和上游临时 URL 自动刷新。

```text
首次：绑定账号 -> 选择 Drive/root -> Delta inventory -> 保存 deltaLink
后续：请求 deltaLink -> 幂等应用变化 -> 原子提交新 deltaLink
```

**v1 范围（已锁定）**：仅 OneDrive Personal（消费 Microsoft 账号 OAuth）。Business / SharePoint **不进入 v1** 正式支持、验收与 Admin 绑定向导；枚举位可保留以便后续版本，运行时必须拒绝。

### 9.4 Filesystem

支持文件系统事件、目录 additions scan、quiet window 和显式 Full validate。`EnableRealtimeMonitor` 仅对声明 file-events capability 的 backend 生效。

### 9.5 游标提交与变更对账

对象变更、新 cursor、递增后的 root `sync_revision` 和对应 `storage_change_outbox` 事件必须在同一数据库事务提交；所有写入的 StorageObject 记录相同 `observed_sync_revision`。处理一页失败时不得提前推进 cursor；重放同一页依靠 provider page identity/dedupe key 幂等，outbox 采用列级契约的 at-least-once 消费。

`Storage Change Reconciler` 以持久化 lease 消费 outbox，并在一个 catalog 事务中：

1. 从 StorageObject presence/revision 派生受影响 MediaLocation availability；
2. 对内容或 revision 变化的 MediaSource 置 Probe `Stale`；可信 ContentIdentity 不一致时 detach Location 并排队重新分类；
3. 对受影响 Series/Movie/Episode 递增对应 structure/source revision；
4. 对 ConfirmedAbsent 执行 Location detach/tombstone，对 TemporarilyUnavailable 只更新可用性；
5. 递增 `catalog_state.generation` 并登记受影响 Item/PlaybackInfo cache invalidation。

SQL catalog transaction 是正确性边界；Redis 删除失败只标记 degraded，generation/revision key 保证旧值不可再命中。Initial、incremental、scoped sync 和 Full Validate 都必须走同一 outbox/reconciler 路径。

---

## 10. Media Scan 与扫描模式

每个 Library 在 SQL `libraries` 行保存 `scan_profile`、`profile_version` 及以下 effective policy；VirtualFolders、Admin 和调度器共用该 SoT：

```text
object_selection_scope
metadata_policy
expansion_policy
probe_policy
```

### 10.1 预设

| 模式 | Object Selection | Metadata | Expansion | Probe |
|------|------------------|----------|-----------|-------|
| Full | all_synced_objects | full | eager | eager |
| Lazy | title_layer | basic | on_browse | on_playback |
| Hybrid | title_layer | basic | background | on_playback |
| Manual | library_roots | none | manual | on_playback |

### 10.2 Full

Full Media Scan 以前置成功的 Storage Inventory/Validate 为依赖，从 SQL 中读取全部已同步对象，再分类 Movie/Series/Season/Episode -> 导入 NFO/图片 -> 完整 metadata -> 建立 MediaSource/Location/Subtitle -> Probe -> 发布。Media Scan 自身不枚举 backend、不写 StorageObject 事实、不推进 cursor。

### 10.3 Lazy

初次：

```text
读取标题层
-> 标题/年份候选匹配
-> 创建轻量 CatalogItem
-> 按配置的 basic metadata 来源尝试获取标题、年份、简介、Provider ID、Primary 海报
-> 不进入 Series/Movie 内部目录
-> 不读取视频内容或轨道
```

首次进入 Movie：由详情请求触发；若子树未物化先等待 scoped Storage Sync，再从 SQL 对象建立版本、Location 和外挂字幕，不 Probe。PlaybackInfo 在 source 尚未索引时复用同一任务并等待。

首次进入 Series：递归该 Series 已同步子树，识别全部 Season/Episode/Location/字幕；使用 publication staging 完整校验后一次切换 active publication，不 Probe。发布成功时所有已发现 Episode 的 `source_state=Indexed`，其 Source/Location/Subtitle 已可用但 Probe 仍为 NotProbed。所有 Expand Item，无论来自用户、Hybrid 后台或 Full 调度，都共用 `(catalog_item_id, task_kind, expected_revision)` 持久化 job 和相同的 staging/原子可见协议；失败不发布半成品。

首次 PlaybackInfo：若 Movie/Episode source 尚未索引，先等待对应 Source Index 任务；然后选择候选 MediaSource，Probe 数据不存在或 stale 时读取必要容器头/尾 Range，写 SQL 后返回。

#### Expand/Index 任务契约

| Item 类型 | 触发 | 任务 | 状态字段 | 原子发布内容 |
|-----------|------|------|----------|--------------|
| Movie | `GET /Items/{id}`；PlaybackInfo 可等待 | Index Media Sources | `source_state`，`structure_state=NotApplicable` | MediaSource、MediaLocation、Subtitle |
| Series | `GET /Items?ParentId=...`；Hybrid/Full | Expand Item | Series `structure_state`；子 Episode `source_state` | Season、Episode 及其 Source/Location/Subtitle；子 Episode 标记 Indexed |
| Episode | 独立条目、Expand 未携带源、storage 变更或管理员 re-index；PlaybackInfo 可等待 | Index Media Sources | `source_state` | MediaSource、MediaLocation、Subtitle |

两个任务都使用 `(catalog_item_id, task_kind, expected_revision)` 持久化 single-flight。Series 子树新增/删除/层级变化递增 Series `structure_expansion_revision` 并排 Expand；某 Episode 的媒体对象、sidecar 或 revision 变化递增该 Episode `source_index_revision` 并排 Index，除非该变化同时改变 Series 结构，此时两者都递增。Series Expand 发布时把每个子 Episode 的 source revision 记录为本次已同步 Storage revision；随后 PlaybackInfo 不重复 Index，只有 source 缺失或 revision stale 才走 Episode 任务。任务提交前必须再次比较 expected/current revision，不一致则丢弃 publication 并重试，禁止发布过期结果。成功发布必须递增 catalog generation。

Library membership 规则：标题层 CatalogItem 显式关联 library；Expand 发布子项时，Season/Episode 继承被展开 Series 当前全部 `library_catalog_items` 关联。Series 从某库移除时，仅移除该库对应子树关联；只要仍被其他库或 MediaSource 引用，就不删除共享 CatalogItem。库根查询走 membership join，树内查询同时校验父项在目标库的 membership。

### 10.4 Hybrid

初次与 Lazy 相同。后台按低优先级展开最近添加、正在观看、收藏、首页候选和管理员指定条目。用户 Expand/PlaybackInfo 优先于后台任务；后台展开仍必须遵守与用户触发相同的 single-flight、完整校验和原子发布。

### 10.5 Manual

Manual 是 root-scoped：初始只注册 `library_storage_roots`，不持久化隐式 selected object 集合。管理员命令必须显式携带 library root 或 CatalogItem scope；Discover Titles 对指定 root 的标题层执行，Expand/Resolve/Probe 对指定 CatalogItem 执行，Full Scan 对指定 root 执行。

### 10.6 Probe 失效

Probe 可以直接针对任一单独 Location 执行，并记录 `probe_location_id + probe_location_revision`；Filesystem 单位置默认使用 backend object revision（canonical path、size、mtime/file ID 的版本组合）判断该 Location 自身结果是否失效，但该组合不具备跨 Location 复用资格。只有 `provider_checksum`、`verified_content_identity` 或 `admin_confirmed` 才允许不同 Location 共享 Probe。Location revision 变化时先使结果 Stale；可信 content identity 不一致时将该 Location 从原 MediaSource 分离并重新分类。管理员可显式重新 Probe。

Probe、Source Indexing 或 Structure Expansion 成功提交后都递增 `catalog_state.generation`，并主动失效受影响 Item/PlaybackInfo key；Redis key 同时可携带 MediaSource probe revision。不得在 TTL 内继续返回 pre-probe 或 pre-expansion DTO。

Probe 只读取识别容器和轨道所需的有界头/尾 Range，不解码帧、不下载完整媒体、不产生视频缓存。

---

## 11. Metadata、身份与来源

- 所有结构化 metadata 写 SQL；NFO 是导入来源，不是运行时 SoT。
- 每个字段可记录 `metadata_provenance`，用于冲突解释和重新匹配。
- Lazy basic metadata 的来源按顺序为已导入/已迁移 SQL metadata、标题层可见 sidecar、已启用的 **TMDb**、命名解析 fallback。title/year 必须尽力产生；overview、Provider ID、Primary 海报在来源不可用时允许为空并将 `metadata_state=Partial`。测试 fixture 必须启用确定性 fake provider 或预置 sidecar，验证完整 basic 字段。
- **v1 远程 provider 仅 TMDb**（Movie/Series/Season/Episode basic 字段与 Primary 图）。TVDB/OMDb/Fanart 等不进入 v1 实现；迁移/NFO 带来的 TVDB/IMDb 等 Provider ID 仍可存储并参与身份匹配。
- 自建进程内 Rust `MetadataProvider` trait + provenance/confidence；**禁止** Jellyfin/Emby 插件宿主、动态 `.so` 插件或直接 vendoring 其 provider 大段源码。
- TMDb 默认关闭：需显式 API key/`enable_remote_providers=true` 才发起远程请求；失败只影响对应条目的 `metadata_state`，不阻止轻量 CatalogItem 发布，也不失败整次 Storage Sync/Media Scan。
- 相同 Provider identity 的作品复用 CatalogItem，但低置信度标题匹配必须进入冲突队列。
- Admin 编辑默认只写 SQL，不回写媒体目录。

---

## 12. 播放与 Proxy Stream

### 12.1 统一链路

```text
客户端 Range
-> TJXY 鉴权/授权
-> 选择 MediaSource
-> 选择可用 MediaLocation
-> StorageBackend.open_range
-> 向上游请求相同 Range
-> streaming body + backpressure 原样转发
```

客户端始终请求 TJXY 的 `/Videos/{itemId}/stream`、`/Audio/{itemId}/stream` 或 OpenAPI 12 字幕路由：

```text
GET /Videos/{itemId}/{mediaSourceId}/Subtitles/{index}/Stream.{format}
GET /Videos/{itemId}/{mediaSourceId}/Subtitles/{index}/{startPositionTicks}/Stream.{format}
```

两条字幕路由都必须鉴权并校验 Item、稳定 MediaSourceId、stream index 和用户库权限。v1 只在 `format` 等于源字幕格式且 `startPositionTicks=0` 时 byte-for-byte 返回；请求转换、时间轴重写或不同格式时返回契约化 400/415。PlaybackInfo 中视频、音频和字幕 URL 都只能是本机鉴权路径，不含云盘 URL 或凭据。

### 12.2 HTTP 语义

实现 GET、HEAD、单 Range、206、416、Content-Range、Content-Length、ETag、If-Range 和条件请求。多 Range v1 明确不支持并返回契约化响应。

### 12.3 背压与取消

- 不完整下载到内存或临时文件。
- 不无限预读，不把 Range 转成整文件读取。
- 下游断开立即取消上游请求。
- Range 重试只在尚未向客户端提交不一致字节且语义安全时进行。
- 播放和扫描使用独立并发池；每个 storage account 有独立限流器。

### 12.4 Location 选择与故障

优先健康、管理员优先级高且支持所需 Range 的 Location。认证过期先刷新；短暂故障可以选择镜像。不得因一次上游失败删除 CatalogItem。

---

## 13. Emby 一键迁移

### 13.1 Adapter 优先级

1. Emby API Importer；
2. NFO/本地图片 Importer；
3. Emby DB Importer，按版本 best-effort。

### 13.2 导入范围

媒体库、Movie/Series/Season/Episode、Provider IDs、标题/简介/年份、People/Genres/Studios、图片、收藏、播放进度、播放次数、已播放状态、旧路径和 Emby Item ID 映射。

### 13.3 Staging 表

```text
import_jobs
import_staging_items
legacy_item_mappings
import_conflicts
import_errors
```

`legacy_item_mappings` 唯一约束：`source_instance_id + legacy_item_id`。新系统生成自己的 CatalogItem ID，不把 Emby ID 当主键。

### 13.4 发布协议

- dry-run 只写 staging 和报告。
- 导入任务可暂停、恢复、重试，重复执行幂等。
- Identity Resolution 后展示冲突和数量核对。
- 正式 publish 在事务/世代协议内原子提交。
- 失败不暴露半迁移 Catalog；publish 失败可回滚。

---

## 14. ScheduledTasks 与优先级

### 14.1 Storage Tasks

```text
Initial Storage Inventory
Scoped Storage Sync
Sync Google Drive Changes
Sync OneDrive Delta
Reconcile Storage Changes
Refresh Storage Credentials
Validate Storage Account
```

### 14.2 Media Tasks

```text
Discover Titles
Resolve Metadata
Expand Item
Index Media Sources
Probe Media
Full Media Scan
Validate Library
Clean Asset Cache
```

### 14.3 优先级

```text
播放 Range 请求         最高，独立池
PlaybackInfo Probe       高
用户触发 scoped sync/Expand/Index 高
云盘增量同步                    中
后台 metadata            低
Hybrid 后台展开          更低
Full Scan               最低
```

播放和扫描不得共享无限制并发池。每个 storage account 配置独立并发上限、速率限制、Retry-After 和指数退避。

保留 Jellyfin 路由：`GET /ScheduledTasks`、`GET /ScheduledTasks/{id}`、`POST/DELETE /ScheduledTasks/Running/{id}`、`POST /Library/Refresh`。

---

## 15. 配置草案

```toml
[server]
bind = "0.0.0.0:8096"
product_name = "TJXY"
version = "0.1.0"
server_name = "Home"

[database]
url = "sqlite://./data/server.db?mode=rwc"

[redis]
mode = "auto"
url = "redis://127.0.0.1:6379"
connect_timeout_ms = 200
key_prefix = "tjxy"

[assets]
dir = "./data/assets"
pregenerate_poster_widths = [300, 480]

[tasks]
expand_wait_timeout_ms = 2500
playback_probe_timeout_ms = 5000
global_scan_concurrency = 4

[proxy]
stream_buffer_bytes = 262144
# 不存在 video_cache_dir、segment_cache 或 offline_cache 配置

[metadata]
enable_remote_providers = false
# v1 唯一远程 provider；未配置 key 时即使 enable=true 也不得请求网络
tmdb_api_key = ""
tmdb_language = "zh-CN"
```

OAuth token 不出现在 TOML；配置只引用加密 credential store。

---

## 16. React Admin

页面范围：

1. 首次启动和登录；
2. 用户、设备、API Keys；
3. 存储账号列表及认证状态；
4. Google Drive OAuth、My Drive/Shared Drive 目录选择；
5. OneDrive OAuth 和目录选择；
6. 账号限流、错误和重新授权；
7. Storage Sync 状态；
8. 媒体库和 Full/Lazy/Hybrid/Manual 模式；
9. 扫描阶段高级配置；
10. 未展开、未匹配和重复候选；
11. CatalogItem 合并/拆分和 Provider ID 修正；
12. MediaSource/MediaLocation；
13. metadata 编辑和重新匹配；
14. Emby 迁移向导、dry-run 和冲突；
15. Proxy 活跃连接和 Storage unavailable 状态；
16. ScheduledTasks、日志摘要和缓存状态。

---

## 17. 实施阶段

### Phase 0：模型和契约

- CatalogItem/MediaSource/MediaLocation、稳定 presentation key 与 library 多对多关系。
- Library effective scan policy、UserData/user revision、work job lease/staging/publication schema。
- StorageBackend interface、contract fake、StorageObject/cursor/change outbox/reconciler schema。
- Assets/import schema、ADR/PD、OpenAPI DTO、字幕路由和 PlaybackInfo flag golden。
- Redis cache-aside、catalog generation 和精确 key layout。

### Phase 1：Filesystem 端到端

- FilesystemBackend、稳定 file ID/weak path move 对账、Full/Lazy/Manual；Hybrid 调度基础。
- Lazy 基础 metadata 及 Partial 降级语义。
- Movie Source Indexing；Series 首次完整原子展开。
- PlaybackInfo 惰性 Probe。
- 本地 GET/HEAD/Range。
- 内容寻址图片。

### Phase 2：Google Drive

- OAuth、My Drive、Shared Drive、目录选择。
- StorageObject、Changes、cursor、scoped sync、outbox reconciliation、限流和重试。
- Range 代理、token 刷新和断连取消。
- Lazy/Hybrid 扫描。
- Strict Lazy 初始标题层 inventory + 访问时 scoped Storage Sync。

### Phase 3：OneDrive

- Microsoft OAuth、OneDrive Personal、Delta。
- Range 代理和临时上游 URL 刷新。
- Business/SharePoint 明确非 v1：模型保留、绑定拒绝、无验收。

### Phase 4：Emby 迁移

- API/NFO/DB adapters。
- Staging、Legacy ID、UserData、图片去重和冲突处理。
- dry-run、恢复、幂等、原子发布。

### Phase 5：客户端兼容与优化

- Findroid 发布门禁、Swiftfin 辅测、Infuse 手工验证。
- Direct Play flag 方言、稳定 MediaSourceId、外挂字幕拉取。
- 多 MediaSource、Lazy Series、Probe 延迟。
- Proxy Range 和云盘故障状态。
- WebSocket LibraryChanged。

---

## 18. 测试策略

### 18.1 StorageBackend contract

所有 adapter 运行同一套：list children、object identity、pagination、changes capability、root/subtree scoped sync、Range、missing object、auth expired、retry、rate limit、cancellation，以及 Present/TemporarilyUnavailable/ConfirmedAbsent 状态转换。单次 404/401/429 不得误确认删除。Filesystem 另测稳定 file ID rename/move 配对、path_weak 候选和不误 purge UserData。

### 18.2 Google Drive

Initial inventory、Changes token、新增/重命名/移动/删除、Shared Drive、token 刷新、429/Retry-After、Range 字节一致。

### 18.3 OneDrive

Initial Delta、nextLink、deltaLink、变更重放、删除、token 失效、临时下载 URL 过期、Range 字节一致。

### 18.4 Lazy

- Strict Lazy 访问未物化子树时只触发一次 scoped Storage Sync；验证 media job 在 sync job Completed、`children_index_revision` 和 `reconciled_sync_revision` 都达到 result revision 前不会启动，且 Media Scan 不直接调用 backend。
- 初次使用确定性 basic metadata fixture，完整字段可用且打开视频字节数为 0；来源缺失时验证 Partial 降级。
- Movie 详情触发 Source Indexing；直接请求 PlaybackInfo 时复用同一任务后再 Probe。
- 首次 Series 只扫描该子树，一次发布全部 Season/Episode。
- Series Expand 发布后 Episode source 已 Indexed，首次 PlaybackInfo 只 Probe；媒体 revision 变化后才单独 Source Index。
- 用户、Hybrid 后台和另一 server instance 并发 Expand 同一 Item 时只有一个持久化 job；其他请求 join，API 超时只读取 active publication。
- worker 崩溃后 lease 可接管并按 natural key 恢复 staging；Storage revision 在任务运行中变化时丢弃旧 publication；成功后 generation bump 且旧缓存立即失效。
- 超大 Series 用分批 staging + 一次 active pointer 切换，对外从不出现部分 Season/Episode。
- 同一 Series 属于多个 library 时，展开子项继承全部 membership；移出一个库不影响其他库。
- Expand 失败不发布半成品且保留可重试状态；成功后的后续访问不再枚举远端目录。

### 18.5 Probe

详情请求只允许 Source Indexing、不 Probe；首次 PlaybackInfo Probe；第二次不访问 backend；并发 single-flight；content identity/revision 变化后重新验证或重 Probe；单 Location 无 checksum 时可使用自身 revision Probe，但不可信镜像不得复用结果；Probe commit 后 pre-probe Redis key 立即失效；失败不声明 Direct Play。

### 18.6 Proxy Stream 与字幕

视频/音频覆盖 GET、HEAD、Range、206、416、Content-Range、Content-Length、ETag、If-Range、断连取消、上游超时、字节一致。外挂字幕测试 OpenAPI 12 两种路由、鉴权/越权、稳定 MediaSourceId/delivery index、源格式 byte-for-byte；验证首次 Probe 的统一 index 分配、re-index 保留、删除后不复用，以及新增内封轨与既有外挂 index 冲突时 container/delivery index 正确分离；不同 format 或非零时间偏移返回 400/415，内封字幕不被提取或转换。断言不创建视频临时文件/segment cache；PlaybackInfo 的媒体和字幕 URL 仅指向 TJXY；DTO、header 和日志不包含真实本地路径、token、Google/Graph 下载域名或上游 URL。

### 18.7 Redis、UserData 与变更对账

有/无 Redis auto、运行中断连、熔断恢复、首页预热、缓存击穿、generation/revision 隔离、空 Lazy 结果短 TTL。验证 UserData upsert 与 `user_catalog_state.revision` 同事务、并发 Favorite/Progress 不丢更新、每个已提交 revision 对应精确 key layout，Redis 删除失败时旧 revision/generation key 不可命中。模拟 Initial/incremental/scoped/Validate 变更，验证 outbox dedupe、lease 超时接管、at-least-once 重放、失败 backoff、连续 reconciled watermark 不越过缺口，以及 Location availability、Probe Stale/detach、item revision、generation 和 cache invalidation 同步推进。断言 Redis 不含凭据、cursor 或视频内容。

### 18.8 Manual 与任务隔离

对四个 scan profile 做 SQL round-trip，验证 VirtualFolders/Admin/调度器读取相同 effective policy，重启后不回落代码默认值。Manual 库初始只注册 root；无显式 root/item scope 的阶段命令返回 4xx；Discover Titles、Expand、Resolve、Probe、Full Scan 按请求 scope 单独执行和重试；播放、Probe、Expand、Sync、后台任务使用受限且相互隔离的并发池。

### 18.9 Emby Import

Dry-run、重复幂等、中断恢复、Legacy ID、图片去重、UserData、冲突、数量核对、publish 回滚。

### 18.10 客户端门禁

Findroid 固定版本自动执行登录 -> 首页 -> 浏览 -> Lazy Series -> 详情 -> PlaybackInfo -> 播放 30 秒 -> 选择外挂字幕并拉取 -> 停止 -> Resume。对 Filesystem 和云端 fixture 保存 PlaybackInfo golden，逐字段验证 `Protocol/Path/DirectStreamUrl/TranscodingUrl/IsRemote/Supports*`，并断言实际传输 byte-for-byte、无 remux/transcode、所有 URL 为 TJXY。Swiftfin 跑同 fixture 辅测。多 MediaSource：断言 PlaybackInfo 可返回完整多源列表且默认源可播；客户端版本 UI 仅 observation，不作为门禁。MediaSource re-index 前后重复该链路，确认 `MediaSourceId` 和字幕 index 稳定。

---

## 19. 性能与可观测性

| 指标 | 目标 |
|------|------|
| Redis 首页 hit P95 | loopback 预热后 < 5ms；SQL counter=0；key 含 catalog/user revision |
| Lazy 初次扫描 | 不打开任何视频内容 |
| Lazy 初次条目 | 配置的 basic 来源可用时显示标题、年份、简介、Provider ID、Primary 海报；来源缺失时 title/year + Partial 状态 |
| Series 首次展开 | 只访问该 Series 子树；分批 staging，最终 active publication 切换事务保持有界 |
| Series 后续访问 | 纯 SQL/Redis，不访问 StorageBackend |
| PlaybackInfo 首次 | 最多一次有界 Range Probe |
| PlaybackInfo 后续 | SQL/Redis 命中，不访问云盘 |
| Filesystem 增量 | 已有媒体不重复 Probe |
| Google 增量 | 正常周期只消费 Changes |
| OneDrive 增量 | 正常周期只消费 Delta |
| Proxy 内存 | 随有界流式缓冲区变化，不随文件大小增长 |
| Proxy 磁盘 | 不产生视频缓存文件 |
| 下游断连 | 立即取消上游，记录取消延迟 |
| 图片 | 相同 SHA-256 只存一份 |
| 作品 | 多库/多路径不重复创建已确认同一 CatalogItem |

关键指标：每账号 API 请求率、429、token 刷新失败、sync lag、cursor age、work job lease 接管/重试/staging rows、Expand/Probe 队列延迟、single-flight join 数、publication 切换耗时、Redis hit rate、proxy active streams、上/下行字节、取消延迟和 storage unavailable 数量。

---

## 20. 开放问题（不得在实现中默认定稿）

### 20.1 Google Drive 初始对象同步 — **已决策：Strict Lazy**

锁定选择：

- **Strict Lazy**：初始只同步标题层；访问时先 scoped/on-demand Storage Sync 将目标子树物化到 SQL，再 Structure Expansion/Source Indexing。
- **否决 Inventory First 作为默认路径**：不得在绑定 Google root 后默认预先物化全树 StorageObject。

决策依据：降低首次 API 成本与 SQL 预取体量；接受首次打开可能增加 scoped sync 延迟；Changes 恢复以标题层基线 + 按需 scope 补齐，不引入隐式全树 inventory。实现必须继续遵守 ADR-013 与 §9.1/§9.2。

### 20.2 多 MediaSource 客户端选择 — **已决策：服务器多源 + 正式默认排序；客户端 UI 非门禁**

锁定选择：

- PlaybackInfo **始终可返回多个 MediaSource**；`MediaSourceId` = 稳定 `presentation_key`。
- §4.4 默认排序升为**正式服务器行为**（非暂行）；无 UI / 未指定源时选排序第一的可 Direct Play 源。
- **L3 门禁（Findroid）**：默认源真播即可；**不要求**版本选择 UI。
- Swiftfin/Infuse 版本 UI 仅 observation，不阻塞发布。
- Admin 可设默认源/优先级/隐藏不可用源。
- **否决**：为弱客户端只返回单 MediaSource。

### 20.3 OneDrive Business / SharePoint v1 范围 — **已决策：v1 仅 Personal**

锁定选择：

- v1 正式支持 **`onedrive_personal` only**。
- `onedrive_business` / `sharepoint`：模型枚举可保留；**禁止** v1 绑定、同步、播放与验收。
- 误用时返回明确 4xx，Admin 文案标明后续版本。
- **否决**：v1 半支持 Business，或用 rclone/FUSE 冒充原生支持。

### 20.4 Metadata Provider 来源 — **已决策：自建 interface + 仅 TMDb 远程**

锁定选择：

- **自建** Rust 进程内 `MetadataProvider` trait、provenance 与 identity resolution。
- **v1 来源顺序**：SQL（含迁移）→ NFO/本地图导入 → **TMDb**（可选）→ 命名 fallback。
- **v1 唯一远程 provider = TMDb**；TVDB/OMDb/Fanart 等默认不实现。
- **永久非目标（v1）**：Jellyfin/Emby 插件运行时、动态插件宿主、未审查地移植其 provider 源码。
- Provider ID 命名空间仍可保存 TMDb/TVDB/IMDb 等键（来自 NFO/迁移/手动），但在线拉取只走 TMDb。

决策依据：TMDb 足以覆盖 Lazy basic 与主海报；缩小许可证与依赖面；保持 scanner 与 metadata 边界清晰。详见 §11 / PD-007。

---

## 21. 风险与缓解

| 风险 | 缓解 |
|------|------|
| 云盘 API 限流 | 每账号并发池、指数退避、Retry-After、播放优先 |
| 认证过期 | 自动刷新、重新授权、CatalogItem 不因单次失败删除 |
| Series 首次展开过慢 | 高优先级、wait timeout、持久化 single-flight、分批 staging + 有界 active publication 切换 |
| 客户端缓存空目录 | 空结果短 TTL、generation bump、P2 WebSocket |
| Jellyfin Direct Play flag 方言差异 | 固定 Findroid/Swiftfin PlaybackInfo golden + 真播门禁；能力仍保持 byte-for-byte |
| 客户端请求字幕转换 | 只广告源格式，非源格式/时间轴重写返回 400/415，不增加转换或 burn-in |
| 标题误匹配 | Provider ID 优先、置信度、低置信度人工确认 |
| 同一作品错误合并 | 可拆分、provenance、禁止弱匹配强制合并 |
| Proxy 带宽压力 | 全局/每账号限制、连接和吞吐观测 |
| 上游断连 | 仅安全条件重试，支持镜像 Location |
| Probe 远程成本 | 首次 PlaybackInfo 才执行，结果持久化 |
| 云盘文件替换 | change outbox/reconciler 使 Location、item revision、Probe 和 generation 同步推进 |
| Filesystem rename identity 弱 | 可靠 file ID 优先；path_weak 只生成 relink 候选，不自动 purge |
| OAuth 泄露 | 加密、日志脱敏、Redis 禁存、不返回直链 |
| Sync 与 Scan 相互阻塞 | 独立队列、优先级和并发池；按需任务只通过 scoped sync 依赖连接 |
| Redis 故障 | cache-aside 回源 SQL，熔断，不作为 SoT |
| 无视频缓存导致重复请求 | 接受取舍，不引入隐式 segment cache |
| Emby 版本差异 | API 优先、DB best-effort、staging 和冲突报告 |

---

## 22. 发布门禁

v2.6 核心原则必须全部可测试：

1. StorageBackend 只负责对象和字节。
2. Storage Sync 只负责远端对象状态。
3. Media Scan 只从 SQL 选择已同步对象；未物化 scope 必须先由 Storage Sync 写 SQL，Media Scan 不推进 cursor 或直接访问 backend。Google root 默认 Strict Lazy：初始仅标题层，访问时 scoped sync，禁止默认全树 Inventory First。
4. SQL Catalog 保存所有结构化 metadata、游标、Probe 和 UserData；UserData commit 同事务递增 user revision。
5. CatalogItem 身份不由路径决定。
6. 已确认相同作品、图片和媒体位置可复用。
7. Lazy 初次提供基础展示且不读取视频。
8. 所有 Series Expand（用户、后台、Full、多实例）使用持久化 lease、幂等 staging 和 active publication 原子切换；子 Episode source 一并 Indexed。
9. 视频信息第一次 PlaybackInfo 才 Probe，Full eager 配置除外。
10. 云盘视频和音频始终由服务器原样 Range 代理；外挂字幕通过钉扎 Jellyfin 路由原格式代理。
11. Findroid 必须用钉扎 PlaybackInfo flag golden 真播 30 秒并成功拉取外挂字幕，且全过程无 remux/transcode；多 MediaSource 时默认源可播即可，客户端版本 UI 非门禁。
12. 客户端 DTO/header 和日志不出现真实本地路径、云盘凭据或临时 URL。
13. MediaSource re-index 后对外 ID 和未变化 stream delivery index 保持稳定；删除的 index 不复用，container/delivery index 冲突有确定映射。
14. Library effective scan policy 持久化于 SQL，VirtualFolders、Admin 和调度器重启前后读取一致。
15. Initial/incremental/scoped/Validate 变更都经列级钉扎、可 lease、at-least-once 的 durable outbox 对账，连续 reconciled sync watermark 不越过失败缺口，并正确推进 Location、Probe、item revision、generation 和缓存隔离。
16. Filesystem 弱路径身份不能自动合并、硬删 CatalogItem 或清除 UserData。
17. 系统不缓存任何视频内容。
18. OneDrive v1 仅 Personal；Business/SharePoint 绑定被拒绝且不进验收。
19. PlaybackInfo 可返回完整多源列表；未指定源时按 §4.4 正式默认排序选择。
