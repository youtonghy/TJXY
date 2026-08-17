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
它可以直接在宿主机运行，也可以通过 Docker Compose 部署，并支持自动管理的
PostgreSQL 或已有外部数据库。

> [!IMPORTANT]
> TJXY 仍在持续开发中，尚未实现完整的 Jellyfin API。在依赖特定 Jellyfin
> 客户端或接口之前，请先查看 [API 兼容性矩阵](docs/api-parity.md)。

## 功能

- **统一媒体目录**：将媒体条目与物理文件、副本、字幕和存储位置分开建模。
- **本地与云端存储**：支持受根目录限制的本地文件、Google Drive、共享云端
  硬盘和 OneDrive Personal，云端凭据不会暴露给浏览器。
- **元数据处理**：从文件名、本地 NFO 和图片发现电影、剧集、单集及音乐，
  并可选择使用 TMDb、MusicBrainz 和 TheAudioDB 补充信息。
- **直接播放**：生成会话级播放地址、按范围传输媒体、选择字幕和文件来源，
  并保存播放状态、收藏及观看进度。
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

## 使用 Docker 快速安装

这是推荐的源码部署方式。启动脚本会构建前端和服务端镜像、创建持久化目录、
启动 Compose 服务，并等待所有服务通过健康检查。

### 环境要求

- Docker Engine 和较新的 Compose v2 插件
- Node.js 22.12 或更高版本及 npm
- 已准备好的 `admin/node_modules` 前端依赖

HeroUI Pro 是受许可保护的构建依赖。首次准备源码的维护者应在 `admin/` 包中
完成安装，且不要把密钥提交到仓库：

```bash
cd admin
HEROUI_KEY="<your-key>" npx -y hpsetup@latest --auto
npm run build
cd ..
```

### 自动管理 PostgreSQL

运行交互式启动脚本：

```bash
./tjxy-setup
```

选择 **Docker 启动** 和 **自动安装并管理 PostgreSQL**，也可以直接传入参数：

```bash
./tjxy-setup \
  --runtime docker \
  --database postgres \
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
| `--media PATH` | `./media` | `/media`，可在媒体库文件浏览器中选择 |
| `--media-mode ro` | `rw` | 以只读方式挂载媒体目录 |
| `--port PORT` | `8096` | 发布 TJXY HTTP 服务端口 |

默认只监听宿主机的 `127.0.0.1`。使用同一台机器上的反向代理或 SSH 隧道时，
建议保持该设置。若需要直接从局域网访问，可以显式监听所有网卡，并通过防火墙
限制端口：

```bash
TJXY_PUBLISH_HOST=0.0.0.0 ./tjxy-setup \
  --runtime docker \
  --database postgres \
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
volume。升级源码部署前，应备份数据库和宿主机目录、停止 TJXY、更新仓库；若
前端锁文件发生变化，还需要重新准备前端依赖，最后使用原启动命令重新启动。

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

打开 `http://127.0.0.1:8096/setup/` 完成安装。默认情况下，安装配置文件会
保存在当前平台的配置目录中；可以通过 `TJXY_CONFIG_FILE` 指定明确路径。

## Linux 发行包

[GitHub Releases](https://github.com/youtonghy/TJXY/releases) 为 glibc 2.35
或更高版本的系统提供 `linux-x86_64-gnu` 和 `linux-aarch64-gnu` 压缩包。
发行包已经包含 Web 资源、服务端和终端工具，因此不需要安装 Rust 或 Node.js
工具链。

```bash
sha256sum -c --ignore-missing SHA256SUMS
tar -xzf tjxy-v0.1.0-linux-x86_64-gnu.tar.gz
cd tjxy-v0.1.0-linux-x86_64-gnu
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
