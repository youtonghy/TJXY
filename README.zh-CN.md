# TJXY

<p align="center">
  <img src="admin/public/brand/tjxy-mark.webp" alt="TJXY" width="128">
</p>

<p align="center">
  面向本地与云端媒体库的自托管 Jellyfin 兼容媒体目录服务。
</p>

<p align="center">
  <a href="README.md">English</a> | <strong>简体中文</strong>
</p>

TJXY 将影视或音乐条目与实际提供文件内容的存储位置分离。同一部电影、
剧集或歌曲可以保持稳定的媒体库身份，同时拥有来自本地磁盘、Google Drive
或 OneDrive 的一个或多个可播放来源。

项目包含浏览器媒体客户端、管理员控制台、Rust HTTP 服务端和终端诊断工具。
推荐安装方式是拉取已经发布的 Docker 镜像。本地构建、源码构建和 Linux
发行包仍可供维护者使用。

> [!IMPORTANT]
> TJXY 仍在持续开发中，尚未实现完整的 Jellyfin API。在依赖特定 Jellyfin
> 客户端或接口之前，请先查看 [API 兼容性矩阵](docs/api-parity.md)。

## 功能

- **统一媒体目录**：将媒体条目与物理文件、副本、字幕和存储位置分开建模。
- **本地与云端存储**：支持受根目录限制的本地文件、Google Drive、共享云端
  硬盘和 OneDrive Personal，云端凭据不会暴露给浏览器。在 Unix 上，同一本地
  根目录的 inode 可验证时，设备号变化不会在重启后替换已有存储身份。
- **元数据处理**：从文件名、本地 NFO 和图片发现电影、剧集、单集及音乐，
  并可选择使用 TMDb、MusicBrainz 和 TheAudioDB 补充信息。
- **直接播放**：生成会话级播放地址、按范围传输媒体、选择字幕和文件来源、复制
  临时直链或调用支持的第三方播放器，并保存站内播放状态、收藏及观看进度。
- **Web 应用**：同一服务提供响应式媒体客户端 `/app/` 和管理控制台 `/admin/`。
- **媒体库任务**：创建媒体库、绑定存储根目录、设置扫描策略、运行持久化扫描
  任务、查看进度并重试失败任务。
- **用户与访问控制**：Argon2id 密码、服务端会话、二维码登录、设备与会话撤销、
  API Key 和管理员用户管理。
- **可选 AI 助手**：连接 OpenAI 兼容服务，在当前用户可见的媒体库和观看记录
  范围内提供媒体发现与推荐。
- **多数据库支持**：可使用 SQLite、PostgreSQL 或 MySQL；`tjxy-setup` 可以自动
  创建并管理 PostgreSQL。
- **运维工具**：健康检查、结构化滚动日志、终端诊断界面、Docker 健康检查和
  持久化安装配置。

## 快速安装

推荐的最终用户安装方式是拉取已经发布的 Docker 镜像。启动脚本会创建持久化
目录、启动 Compose 服务，并等待所有服务通过健康检查。部署主机不需要
Node.js、Rust 或 `HEROUI_KEY`。

### 环境要求

- Docker Engine 和较新的 Compose v2 插件

### 使用已发布 Docker 镜像

当前版本为 `ghcr.io/youtonghy/tjxy:0.0.1`（`linux/amd64` 和 `linux/arm64`）。
`latest` 标签指向同一镜像。生产环境请固定使用版本标签。

仓库和 GHCR 镜像包目前是私有的，首次拉取前需要先登录：

```bash
docker login ghcr.io
./tjxy-setup \
  --runtime docker \
  --database postgres \
  --image ghcr.io/youtonghy/tjxy:0.0.1 \
  --media /path/to/media
```

`TJXY_IMAGE=ghcr.io/youtonghy/tjxy:0.0.1` 与 `--image` 等价。更新时重新执行
同一命令即可拉取新标签并重建应用容器。

### 自动管理 PostgreSQL

上面的命令会自动创建 PostgreSQL。也可以运行交互式启动脚本，同时传入已发布
镜像：

```bash
./tjxy-setup --image ghcr.io/youtonghy/tjxy:0.0.1
```

选择 **Docker 启动** 和 **自动安装并管理 PostgreSQL**，也可以直接传入参数：

```bash
./tjxy-setup \
  --runtime docker \
  --database postgres \
  --image ghcr.io/youtonghy/tjxy:0.0.1 \
  --media /path/to/media \
  --port 8096
```

若未指定其他路径，脚本会把配置保存在 `.tjxy/config`，把应用数据保存在
`.tjxy/data`。在 Linux 上，请使用将来负责这些目录的账号运行脚本。启动完成后
打开 `http://127.0.0.1:8096/setup/`，创建第一个管理员账号。

PostgreSQL 密码会自动生成并保存在 `.tjxy/postgres-password`，不会打印到
终端，也不会发送到浏览器。数据库只在 Compose 内部网络中可用。停止应用后，
数据仍保存在 `tjxy-setup_tjxy-postgres` Docker volume 中。

### 使用外部数据库

选择 external 模式即可使用已有的 SQLite、PostgreSQL 或 MySQL：

```bash
./tjxy-setup \
  --runtime docker \
  --database external \
  --image ghcr.io/youtonghy/tjxy:0.0.1 \
  --media /path/to/media
```

在浏览器安装向导中填写数据库信息。若数据库运行在 Docker 宿主机上，应使用
`host.docker.internal`，不能使用 `localhost`。Compose 已为 Linux 添加
`host-gateway` 映射，同时兼容 Docker Desktop。

### 存储目录与端口

| 参数 | 默认值 | 容器路径或行为 |
| --- | --- | --- |
| `--config-dir PATH` | `.tjxy/config` | `/config`，包含 `tjxy.toml` |
| `--data-dir PATH` | `.tjxy/data` | `/data`，包含资源和日志 |
| `--media PATH` | `./media` | `/media`，可供媒体库和 STRM 目标访问 |
| `--media-mode ro` | `rw` | 以只读方式挂载媒体目录 |
| `--port PORT` | `8096` | 发布 TJXY HTTP 服务端口 |
| `--image IMAGE` | 未设置时从源码构建 | 推荐使用 `ghcr.io/youtonghy/tjxy:0.0.1`。仅在从当前源码构建时省略 |

默认只监听宿主机的 `127.0.0.1`。使用同一台机器上的反向代理或 SSH 隧道时，
建议保持该设置。若需要直接从局域网访问，可以显式监听所有网卡，并通过防火墙
限制端口：

```bash
TJXY_PUBLISH_HOST=0.0.0.0 ./tjxy-setup \
  --runtime docker \
  --database postgres \
  --image ghcr.io/youtonghy/tjxy:0.0.1 \
  --port 8096
```

在受信任网络以外开放 TJXY 前，请配置 TLS 反向代理。

### 运维与升级

```bash
./tjxy-setup status
./tjxy-setup logs
./tjxy-setup stop
```

`stop` 会删除容器和 Compose 网络，但不会删除宿主机挂载文件或 PostgreSQL
volume。升级发布镜像部署前，应先备份数据库和宿主机目录，再把原命令中的镜像
版本改为新版本并重新执行；脚本会先拉取镜像，然后重建 TJXY。

除非确定要永久删除自动管理的 PostgreSQL 数据，否则不要运行
`docker compose down --volumes`。

## 本地安装

本地构建需要 Rust 1.88 或更高版本、Node.js 22.12 或更高版本、npm，以及
已经准备好的前端依赖。如果本地运行 TJXY、同时使用自动管理 PostgreSQL，
还需要 Docker。

使用启动脚本进行本地安装：

```bash
./tjxy-setup --runtime local --database external --media /path/to/media
```

让 TJXY 在本地运行，同时通过 Docker 自动创建 PostgreSQL：

```bash
./tjxy-setup \
  --runtime local \
  --database postgres \
  --media /path/to/media \
  --postgres-port 5433
```

手动构建方式：

```bash
npm --prefix admin run build
cargo build --release --locked -p tjxy-server --bin tjxy-server
TJXY_ADMIN_DIST_DIR=admin/dist ./target/release/tjxy-server
```

本地构建默认显示产品版本 `0.0.0`。需要手动写入构建版本时，应向前后端传入同一
版本：

```bash
VITE_TJXY_VERSION=0.2.0 npm --prefix admin run build
TJXY_BUILD_VERSION=0.2.0 cargo build --release --locked -p tjxy-server --bin tjxy-server
```

可以额外通过 `VITE_TJXY_COMMIT` 将构建提交哈希显示在管理后台的“关于”页面。

发布工作流会自动注入已经校验的发布版本。

打开 `http://127.0.0.1:8096/setup/` 完成安装。默认情况下，安装配置文件会
保存在当前平台的配置目录中；可以通过 `TJXY_CONFIG_FILE` 指定明确路径。

当前版本完成的工作任务默认保留 7 天。可将
`TJXY_WORK_HISTORY_RETENTION_DAYS` 设置为 1 至 3650，或通过
`TJXY_WORK_HISTORY_RETENTION_ENABLED=false` 暂停保留。保留 worker 每次最多登记 1,000 条旧版本
遗留的终态任务，并以每事务最多 100 条的短批次清理。PostgreSQL 和 SQLite 的任务领取索引只保留
Pending/Running 任务。PostgreSQL 会在迁移事务内替换索引，因此任务历史较大时，升级后的首次启动
可能需要更长时间。历史已处理 outbox 也会由后台分批清理；storage 事件进入 dead-letter 后保留 7 天。

删除记录不会立即缩小现有数据库文件。大规模清理后可在正常运行期间执行
`VACUUM (ANALYZE)`；若需要将空间归还给文件系统，应单独安排维护窗口执行 `VACUUM FULL`，
或使用 pg_repack 等 PostgreSQL 在线重整工具。

### Jellyfin 客户端播放兼容性

TJXY 实现的是 Jellyfin 的原文件 Direct Play 子集，不提供转封装或转码。
PlaybackInfo 接受可选或空的 POST body，并忽略 Jellyfin 服务端模型绑定同样会
忽略的客户端查询提示。原文件接口兼容 Jellyfin 使用的小写 `/videos`、`/audio`
路径和可选容器后缀；已认证请求缺少 `MediaSourceId` 时，会选择当前用户有权访问的
第一个可播放来源。`stream.mp4` 之类的后缀仅用于路由兼容，不会转换文件或改变真实
响应 MIME。`static` 缺失或为 false 时仍交付原文件；这些 progressive 接口会将常见
转码查询参数作为兼容提示接受，但仍返回原始字节和真实 MIME。仅使用 PlaybackTicket
的请求仍必须携带票据绑定的明确媒体来源。

播放状态接口接受缺失或 `null` 的 `PositionTicks`，并将 `MediaSourceId=ItemId`
解释为默认媒体来源。`Playing`、`Progress` 或 `Stopped` 实际更新观看记录后，服务端
会向当前用户发布 `UserDataChanged`；重复事件保持幂等，不会产生额外 revision 或通知。

TJXY 浏览器客户端只声明浏览器稳定支持的 Direct Play 容器，因此不声明 MKV；桌面
客户端会使用本地 FFmpeg 将浏览器不能直接解码的文件转换为完整的 VOD HLS 清单，
并保留有限时长与可拖动进度。移动端只选择设备原生可播放的来源。TJXY 仍不提供
服务端 HLS manifest、直播源或通用转码服务，也不会承诺客户端能够解码不支持的容器
或编解码器。

## Linux 发行包

[GitHub Releases](https://github.com/youtonghy/TJXY/releases) 为 glibc 2.35
或更高版本的系统提供 `linux-x86_64-gnu` 和 `linux-aarch64-gnu` 压缩包。
发行包已经包含 Web 资源、服务端和终端工具，因此不需要安装 Rust 或 Node.js
工具链。

```bash
sha256sum -c --ignore-missing SHA256SUMS
tar -xzf tjxy-v0.0.1-linux-x86_64-gnu.tar.gz
cd tjxy-v0.0.1-linux-x86_64-gnu
./tjxy
```

终端工具可以启动、停止、重启和检查服务端。按 `g` 可以切换中文和英文。

## 首次启动配置

在安装配置文件创建之前，TJXY 只提供安装页面。安装向导用于配置品牌信息、
必要时的数据库连接、网络设置以及第一个管理员账号。完成安装后：

- `/app/` 提供普通用户媒体客户端；
- `/admin/` 提供管理员控制台；
- `/health/ready` 返回应用与数据库就绪状态；
- 安装页面地址会重定向到已安装的应用。

完成后的配置中保存了安装身份和数据库地址。在本地与 Docker 之间切换，或者
在自动管理与外部数据库之间切换时，不要复用同一个 `tjxy.toml`。
`tjxy-setup` 会检测这种情况，并要求使用新的 `--config-dir`，不会静默修改
已有安装。

## 从源码构建

源码构建面向维护者。需要 Node.js 22.12 或更高版本、npm、已经准备好的
`admin/node_modules` 前端依赖；编译服务端时还需要 Rust 1.88 或更高版本。
HeroUI Pro 是受许可保护的构建依赖。发布版本前，必须将 `HEROUI_KEY` 保存为
GitHub Actions repository secret。本地准备源码时也可以在 `admin/` 中安装，
但不要把密钥提交到仓库：

```bash
cd admin
HEROUI_KEY="<your-key>" npx -y hpsetup@latest --auto
npm run build
cd ..
```

`Release` workflow 只在生成 `admin/dist` 时使用该 secret。已发布镜像和
Linux 发行包已经包含构建好的前端，部署主机永远不需要这个密钥。若要从当前
源码构建 Docker 而不是拉取 GHCR 镜像，请省略 `--image` 和 `TJXY_IMAGE`：

```bash
./tjxy-setup --runtime docker --database postgres --media /path/to/media
```

手动发布时，进入 **Actions > Release > Run workflow**，输入例如 `0.0.1` 或
`v0.0.1`。CI 会直接构建当前 `main`，并创建 tag、GitHub Release、便携发行包
和容器镜像，不需要提前创建 tag。继续支持直接推送 `vX.Y.Z` tag；这种方式仍会
严格检查 Cargo workspace 版本。

## 开发

服务端是基于 Axum 和 SeaORM 的 Rust 2024 workspace。前端使用 React 19、
HeroUI v3、Tailwind CSS v4 和 Vite。

```bash
# 前端
npm --prefix admin run typecheck
npm --prefix admin run lint
npm --prefix admin test -- --run
npm --prefix admin run build

# Rust workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

数据库测试默认使用 SQLite。运行跨数据库测试时，可以通过
`TJXY_TEST_DATABASE_URL` 指向专用的 PostgreSQL 或 MySQL 测试实例。每个测试
都会创建独立的数据库或 schema。

## 文档

- [API 兼容性矩阵](docs/api-parity.md)
- [主题开发](docs/themes.md)
- [实现计划](PLAN.md)
- [English README](README.md)

## 许可证

TJXY 使用 [MIT License](LICENSE)。
