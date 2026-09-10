# TMP 本地扫库、播放与性能复核

本轮使用 `/tmp/tjxy-local-validation-v2` 中四个独立 SQLite 数据库，没有操作已有安装数据库。8 项库使用可实际解码的 12 秒 1280×720 H.264/AAC 视频（每份 6,267,913 字节）；274/2740 项库使用极小 MP4、NFO 和 PNG，适合测目录与调度成本，不能代表真实大视频吞吐。服务为 Rust 1.88.0 release 构建；测量期间主机存在其他测试负载，未清空系统页缓存。

## 已完成结果

| 数据集 | 两次完整扫描 | 峰值服务 RSS | 20 并发列表 / 海报 / PlaybackInfo P95 |
|---|---|---|---|
| 8 项，720p | 93.91 / 95.20 秒 | 60.13 MiB | 10.26 / 11.16 / 33.34 ms |
| 274 项 | 132.47 / 157.05 秒 | 58.86 MiB | 13.75 / 9.52 / 32.18 ms |

两库扫描结果均按实际计数核对：每次 `items` 等于库规模，`failed=0`、`needs_selection=0`。274 个媒体源全部为 Probed。两次扫描期间源文件摘要不变，三张图片资产表和图片资产文件数均为 0。8 项库随后有意修改 NFO、海报和第二个视频，作为变更测试。

- 浏览器先后两组各 10 次播放，均实际收到解码帧，暂停和跳到第 8 秒后续播通过。修复后首个样本 96.9 ms，其余 9 次 73.5–241.1 ms；这是回环、短片、缓存可能已热的样本，不能称为磁盘冷启动或生产首帧指标。
- 64 KiB Range 请求分别以 1/5/20 并发验证 20/25/100 次，每次响应字节与源文件逐字节匹配，P95 为 3.38/4.35/8.80 ms。验证 206、HEAD 无正文且长度正确、越界 Range 416、图片条件请求 304。
- 服务重启后可播放，元数据修改持久化。删除源海报返回 404；重新创建文件改变底层文件身份，复扫前返回 503，Validate + Resolve 后恢复 200 且正文一致。这条路径需要重新同步文件身份。
- 第一项 NFO 年份 2001→2003 后仅该项更新，其余年份保持 2001。第二项视频替换为 3 秒 640×360 后，重新索引与探测得到正确宽高和时长。
- 最终 release 再次完整复扫变更后的 8 项库，用时 95.18 秒，8/8 媒体源为 Probed；之后 HTTP 字节、Range、HEAD、304 与零资产检查全部通过，20 并发 Range P95 为 10.30 ms。
- 四库 `PRAGMA quick_check` 均为 `ok`，`foreign_key_check` 均无违规。

空闲 15 秒样本：8 项/274 项的单核 CPU 为 0.13%/0.20%，SQL 语句为 2.80/3.13 条每秒；领取查询约 1.40 次每秒。任务创建至首次领取 P95 为 38.02 ms/27.34 秒，后者包含批量任务排队时间，不能当成通知唤醒延迟。实际领取计划使用 `ix_work_jobs_claim_active`，仍有 ORDER BY 临时 B-tree。

274 项数据库两轮扫描后的分配空间分别约 4.52/6.05 MiB；历史结果、publication 和同步记录仍增长。短测没有跨完整保留周期，不能据此宣称长期零增长。SQL 是驱动语句计数，不是事务率。

2740 项扫描进行期间，另对列表、海报和一个已探测媒体的 PlaybackInfo 各发送 100 次、20 并发请求，全部验证 HTTP 状态和正文。P95 分别为 27.68/12.36/46.74 ms。这证明该时段交互接口可用，不代表整个媒体库已完成扫描。

## 本轮发现与修复

1. 有效本地 NFO 没有远程 provider ID 时，被持久化为 Partial，详情页最终误报“元数据仍不可用”。本地模式已按有效 NFO 结果标为 Ready，同时保留远程详情未加载的事实。无 provider ID 的回归用例、22 项元数据契约及浏览器页面复核通过。
2. 2740 项首次运行暴露扫描竞态：先读取空目标集合、后读取已完成的发现任务，可能报告 `items=0` 完成。将目标读取移到前置任务完成检查之后；基准脚本增加每轮实际计数与失败数断言，并增加媒体源探测状态及 HTTP 正文断言，防止仅凭 Completed/200 报通过。

原 2740 项失败运行保留在 `db-2740-report.json`：1200 秒停止，首次扫描的 0 项结果不算通过。修复后新建的 2740 项库同样在 1200.08 秒停止，退出码 1，首轮扫描尚未完成，没有再生成 0 项成功结果。停止时 2740 个目录已入库，但媒体源仅 1061 个，其中 15 个 Probed，其余尚未完成；以最终 `catalog-audit.json` 为准。这一级未通过容量验收，也未完成第二轮与扫描后并发矩阵；没有继续扩大到 27400/82200 项。

## 验证与复现

原始 JSON 与本文同目录；不包含测试账户口令或播放凭据。浏览器数据在 `browser-*.json`，HTTP 与源变化数据在 `http-after-restart.json`、`source-change.json`、`video-source-change.json`。

```sh
cargo +1.88.0 build -p tjxy-server --example capacity_server --release --locked
python3 scripts/capacity/run.py --root /tmp/tjxy-new-local --items 8 --media-file /path/to/local.mp4 --concurrency 1 5 20 --max-seconds 1200 --server-binary target/release/examples/capacity_server --report /tmp/tjxy-new-local.json
# 基准进程结束后，可单独重启其隔离服务，再运行 HTTP 校验。
target/release/examples/capacity_server /tmp/tjxy-new-local 8 600 restart
python3 scripts/capacity/verify_local.py --root /tmp/tjxy-new-local --report /tmp/tjxy-new-local-http.json
```

`--mutate` 仅适用于新建的确定性 fixture，会改变测试源文件，不能重复假定初始年份未变。报告 `stopped` 或命令非零退出均不是通过。

本轮没有执行远程存储、4K/高码率、长视频、转码、跨天稳定性或更大规模容量测试。前一轮前端 546 项及 PostgreSQL/MySQL 各 113 项结果见相邻的 2026-09-09 报告，本轮没有改前端或迁移，不把历史记录算成本轮重跑。

浏览器实际播放画面：

![720p 测试视频实际播放](playback.png)

最终代码验证：Rust 1.88.0 全工作区测试退出 0，902 通过、0 失败、2 个仓库原有环境依赖忽略项；Clippy 全工作区全部 targets（`-D warnings`）、rustfmt、Python 编译检查及 Git diff 空白检查均退出 0。本轮没有创建 commit。

2740 项扫描期间额外执行 3 秒 macOS `sample`，活跃数据库线程主要在 `sqlite3VdbeExec`、B-tree 与记录比较中。此采样仅把耗时定位到 SQLite 执行，尚未锁定具体 SQL 或完成性能修复；原始调用栈保留在 TMP 的 `large-scan-sample.txt`。

修复后 2740 项停止时峰值服务 RSS 为 77.16 MiB；停止原因是时间预算，并非内存上限。基准服务已经停止，TMP 数据库保留。
