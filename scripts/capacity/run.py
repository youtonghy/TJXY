#!/usr/bin/env python3
"""Run bounded, isolated local capacity measurements against the real TJXY worker/API stack.

Build the example first. Never supply a production URL or database: the server owns a new
private directory and a loopback-only port. Reports deliberately leave browser first-frame
and remote-provider metrics null until separately measured.
"""
import argparse
import concurrent.futures
import hashlib
import json
import os
import shutil
from pathlib import Path
import sqlite3
import subprocess
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid


def read_json(path):
    return json.loads(Path(path).read_text())


def distribution(samples):
    ordered = sorted(samples)
    if not ordered:
        return None
    return {"samples": len(ordered), "p50_ms": ordered[(len(ordered)-1)//2],
            "p95_ms": ordered[min(len(ordered)-1, int(len(ordered)*0.95))], "max_ms": ordered[-1]}


def cpu_seconds(value):
    days, _, clock = value.rpartition("-")
    result = 0
    for part in clock.split(":"):
        result = result * 60 + float(part)
    return result + float(days or 0) * 86400


class Run:
    def __init__(self, args):
        self.args = args
        self.root = Path(args.root).resolve()
        if self.root.exists():
            raise ValueError("The fixture directory must not exist")
        self.started = time.monotonic()
        self.deadline = self.started + args.max_seconds
        self.stop_reason = None
        self.peak_rss_kib = 0
        self.peak_disk_kib = 0
        self.last_disk_sample = 0
        self.token = None
        self.base = None
        self.server = None
        self.finished = threading.Event()
        self.report = {"items": args.items, "concurrency": args.concurrency, "scan_concurrency_mode": args.scan_concurrency, "backend": "SQLite",
                       "fixture": "deterministic local MP4/NFO/PNG", "browser_first_frame": None,
                       "remote_probe": None, "actual_transaction_rate": None, "status": "running"}

    def guard(self):
        if self.stop_reason:
            raise RuntimeError(self.stop_reason)
        if time.monotonic() >= self.deadline:
            raise TimeoutError("total time budget exhausted")
        if self.server and self.server.poll() is not None:
            raise RuntimeError("isolated server exited")

    def request(self, path, body=None, method=None, raw=False):
        self.guard()
        headers = {"Authorization": 'MediaBrowser Client="Capacity", Device="Local", DeviceId="capacity", Version="1"'}
        if self.token:
            headers["Authorization"] += f', Token="{self.token}"'
        data = None if body is None else json.dumps(body).encode()
        if data is not None:
            headers["Content-Type"] = "application/json"
        request = urllib.request.Request(self.base + path, data=data, headers=headers, method=method)
        started = time.monotonic()
        try:
            with urllib.request.urlopen(request, timeout=min(15, max(0.1, self.deadline-time.monotonic()))) as response:
                payload = response.read(4 * 1024 * 1024)
        except urllib.error.HTTPError as error:
            raise RuntimeError(f"HTTP {error.code} at {path.split('?')[0]}") from None
        elapsed = (time.monotonic()-started)*1000
        return (payload if raw or not payload else json.loads(payload)), elapsed

    def rows(self, sql, parameters=()):
        self.guard()
        started = time.monotonic()
        while True:
            self.guard()
            try:
                with sqlite3.connect(f"file:{self.root}/catalog.db?mode=ro", uri=True, timeout=1) as db:
                    return [tuple(str(uuid.UUID(bytes=value)) if isinstance(value, bytes) and len(value) == 16 else value for value in row) for row in db.execute(sql, parameters).fetchall()]
            except sqlite3.OperationalError as error:
                if "locked" not in str(error).lower() or time.monotonic() - started >= 30:
                    raise
                self.report["measurement_lock_retries"] = self.report.get("measurement_lock_retries", 0) + 1
                time.sleep(0.2)

    def resource_sample(self):
        output = subprocess.check_output(["ps", "-o", "rss=,time=", "-p", str(self.server.pid)], text=True).strip().split()
        return int(output[0]), cpu_seconds(output[1])

    def monitor(self):
        disk_failures = 0
        while not self.finished.wait(1):
            try:
                rss, _ = self.resource_sample()
                self.peak_rss_kib = max(self.peak_rss_kib, rss)
                if time.monotonic() - self.last_disk_sample >= 10 and self.root.exists():
                    try:
                        disk = int(subprocess.check_output(["du", "-sk", str(self.root)], text=True, stderr=subprocess.DEVNULL).split()[0])
                    except subprocess.CalledProcessError:
                        # SQLite can unlink a transient journal while du walks it.
                        # Keep RSS/time guards alive and retry the disk sample.
                        disk_failures += 1
                        self.report["disk_sample_retries"] = self.report.get("disk_sample_retries", 0) + 1
                        if disk_failures >= 3:
                            self.stop_reason = "disk sampling repeatedly unavailable"
                    else:
                        disk_failures = 0
                        self.peak_disk_kib = max(self.peak_disk_kib, disk)
                        if disk > self.args.max_disk_mib*1024:
                            self.stop_reason = "disk budget exhausted"
                    self.last_disk_sample = time.monotonic()
                if rss > self.args.max_rss_mib*1024:
                    self.stop_reason = "RSS budget exhausted"
                if time.monotonic() >= self.deadline:
                    self.stop_reason = "total time budget exhausted"
                if self.stop_reason:
                    self.server.terminate()
                    return
            except (subprocess.CalledProcessError, IndexError):
                return
            except OSError:
                self.stop_reason = "resource sampling unavailable"
                self.server.terminate()
                return

    def source_digest(self):
        digest = hashlib.sha256()
        for path in sorted((self.root/"media").rglob("*")):
            if path.is_file():
                digest.update(str(path.relative_to(self.root/"media")).encode())
                digest.update(path.read_bytes())
        return digest.hexdigest()

    def snapshot(self):
        tables = ["catalog_items", "asset_blobs", "item_assets", "person_assets", "direct_metadata_refs", "work_jobs", "work_results", "work_staging_rows", "storage_sync_pages", "catalog_publications"]
        counts = {table: self.rows(f'SELECT COUNT(*) FROM "{table}"')[0][0] for table in tables}
        with sqlite3.connect(f"file:{self.root}/catalog.db?mode=ro", uri=True) as db:
            pages = db.execute("PRAGMA page_count").fetchone()[0]
            size = db.execute("PRAGMA page_size").fetchone()[0]
            free = db.execute("PRAGMA freelist_count").fetchone()[0]
            try:
                logical = dict(db.execute("SELECT name,SUM(payload) FROM dbstat GROUP BY name"))
            except sqlite3.OperationalError:
                logical = None
        wal = self.root/"catalog.db-wal"
        return {"rows": counts, "allocated_bytes": pages*size, "free_bytes": free*size,
                "wal_file_bytes": wal.stat().st_size if wal.exists() else 0, "payload_by_table_and_index": logical,
                "asset_files": sum(1 for p in (self.root/"assets").rglob("*") if p.is_file())}

    def wait_job(self, job):
        while True:
            self.guard()
            rows = self.rows("SELECT state,last_error FROM work_jobs WHERE id=?", (uuid.UUID(job).bytes,))
            # Support both native UUID BLOBs and older canonical TEXT fixtures.
            if not rows:
                rows = self.rows("SELECT state,last_error FROM work_jobs WHERE id=?", (job,))
            if rows and rows[0][0] == "Completed":
                return
            if rows and rows[0][0] == "Failed":
                raise RuntimeError("scan ended in failure: " + str(rows[0][1])[:300])
            time.sleep(0.15)

    def measure_requests(self, user, library, items):
        phases = []
        for concurrency in self.args.concurrency:
            phase = {"concurrency": concurrency}
            paths = {
                "list": lambda _: (f"/Items?userId={user}&parentId={library}&recursive=true&includeItemTypes=Movie&limit=20", None),
                "poster": lambda index: (f"/Items/{items[index % len(items)]}/Images/Primary", None),
                "playback_info": lambda index: (f"/Items/{items[index % len(items)]}/PlaybackInfo?userId={user}", {"DeviceProfile":{"DirectPlayProfiles":[{"Type":"Video","Container":"mp4"}]}}),
            }
            for label, make in paths.items():
                def sample(index):
                    path, body = make(index)
                    payload, elapsed = self.request(path, body, raw=(label == "poster"))
                    if label == "playback_info" and not payload.get("MediaSources"):
                        raise RuntimeError("playback preparation returned no playable sources")
                    if label == "list" and len(payload.get("Items", [])) != min(20, len(items)):
                        raise RuntimeError("catalog page omitted expected fixture items")
                    if label == "poster" and not payload.startswith(b"\x89PNG\r\n\x1a\n"):
                        raise RuntimeError("poster response is not the fixture PNG")
                    return elapsed
                with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as executor:
                    phase[label] = distribution(list(executor.map(sample, range(max(20,concurrency*5)))))
            phases.append(phase)
            self.guard()
        self.report["api_latency"] = phases

    def execute(self):
        log = self.root.parent/(self.root.name+".process.log")
        with log.open("xb") as output:
            environment = dict(os.environ, TJXY_SCAN_CONCURRENCY=self.args.scan_concurrency)
            self.server = subprocess.Popen([str(Path(self.args.server_binary).resolve()), str(self.root), str(self.args.items), str(self.args.max_seconds)], stdout=output, stderr=output, env=environment)
        monitor = threading.Thread(target=self.monitor, daemon=True)
        monitor.start()
        try:
            connection = self.root/"connection.json"
            while not connection.exists():
                self.guard(); time.sleep(0.1)
            configuration = read_json(connection)
            self.base = configuration["base_url"]
            auth, _ = self.request("/Users/AuthenticateByName", {"Username":configuration["username"], "Pw":configuration["password"]})
            self.token = auth["AccessToken"]
            user = auth["User"]["Id"]
            if self.args.media_file:
                media = Path(self.args.media_file).resolve()
                if not media.is_file() or not 0 < media.stat().st_size <= 16 * 1024 * 1024:
                    raise ValueError("media fixture must be a nonempty file of at most 16 MiB")
                if media.stat().st_size * self.args.items > self.args.max_disk_mib * 1024 * 1024 // 2:
                    raise ValueError("media fixture would exceed half the disk budget")
                for target in (self.root/"media").rglob("*.mp4"):
                    shutil.copyfile(media, target)
                self.report["media_fixture_bytes"] = media.stat().st_size
                self.report["media_fixture_sha256"] = hashlib.sha256(media.read_bytes()).hexdigest()
                self.report["fixture"] = "supplied local MP4 with deterministic NFO/PNG"
            self.report["before_library"] = self.snapshot()
            source_digest = self.source_digest()
            query = urllib.parse.urlencode({"name":"Capacity", "collectionType":"movies", "paths":str(self.root/"media"), "refreshLibrary":"false"})
            self.request("/Library/VirtualFolders?"+query, {"LibraryOptions":{"Enabled":True, "ScanProfile":"Full", "MetadataSourceMode":"local_only", "LocalMetadataAccessMode":"import_metadata_only"}})
            library, root = self.rows("SELECT library_id,storage_root_id FROM library_storage_roots LIMIT 1")[0]
            scans = []
            self.report["scans"] = scans
            for iteration in range(2):
                started = time.monotonic()
                _, cpu_before = self.resource_sample()
                submission, _ = self.request(f"/Admin/Tasks/FullScan/{library}/{root}", {}, "POST")
                self.report["active_scan"] = {"iteration": iteration + 1, "job_id": submission["JobId"]}
                self.wait_job(submission["JobId"])
                duration = time.monotonic()-started
                _, cpu_after = self.resource_sample()
                result = self.rows("SELECT counters FROM work_results WHERE job_id=?", (uuid.UUID(submission["JobId"]).bytes,))
                counters = json.loads(result[0][0]) if result else {}
                scans.append({"iteration":iteration+1, "duration_seconds":duration,
                              "server_cpu_percent_one_core":(cpu_after-cpu_before)/duration*100,
                              "result":counters,
                              "snapshot":self.snapshot()})
                self.report.pop("active_scan", None)
                if counters.get("items") != self.args.items or counters.get("failed", 0) or counters.get("needs_selection", 0):
                    raise RuntimeError("scan result did not successfully account for every fixture item")
            items = [row[0] for row in self.rows("SELECT id FROM catalog_items WHERE item_type='Movie' ORDER BY name")]
            if len(items) != self.args.items:
                raise RuntimeError(f"expected {self.args.items} movies, found {len(items)}")
            self.report["probed_sources"] = self.rows("SELECT COUNT(*) FROM media_sources WHERE probe_state='Probed'")[0][0]
            if self.report["probed_sources"] != self.args.items:
                raise RuntimeError("not every fixture movie has a successfully probed source")
            self.report["source_unchanged"] = self.source_digest() == source_digest
            waits = self.rows("SELECT (julianday(started_at)-julianday(created_at))*86400000 FROM work_jobs WHERE started_at IS NOT NULL AND started_at >= created_at")
            stage_rows = self.rows("SELECT task_kind,state,COUNT(*),SUM((julianday(completed_at)-julianday(started_at))*86400000) FROM work_jobs GROUP BY task_kind,state")
            self.report["job_stages"] = [{"kind": kind, "state": state, "jobs": count, "sum_first_claim_to_completion_ms": elapsed} for kind, state, count, elapsed in stage_rows]
            self.report["job_created_to_first_claim"] = distribution([row[0] for row in waits if row[0] is not None])
            self.measure_requests(user, library, items)
            self.report["after_requests"] = self.snapshot()
            if any(self.report["after_requests"]["rows"][table] for table in ["asset_blobs","item_assets","person_assets"]):
                raise RuntimeError("zero-copy invariant violated")
            if self.args.idle_seconds:
                # Allow the queue to reach its capped backoff before sampling idle work.
                time.sleep(10)
                self.guard()
                before_sql = read_json(self.root/"metrics.json")
                _, before_cpu = self.resource_sample()
                idle_started = time.monotonic()
                time.sleep(self.args.idle_seconds)
                self.guard()
                _, after_cpu = self.resource_sample()
                after_sql = read_json(self.root/"metrics.json")
                seconds = time.monotonic()-idle_started
                differences = {key:{field:value[field]-before_sql.get(key,{}).get(field,0) for field in ("statements", "elapsed_us", "failures")} for key,value in after_sql.items()}
                self.report["idle"] = {"seconds":seconds, "server_cpu_percent_one_core":(after_cpu-before_cpu)/seconds*100,
                                       "sql":differences, "sql_statements_per_second":sum(v["statements"] for v in differences.values())/seconds,
                                       "queue_claims_per_second":differences.get("queue_claim",{}).get("statements",0)/seconds}
            else:
                self.report["idle"] = None
            self.report["queue_claim_plan"] = read_json(self.root/"claim-plan.json") if (self.root/"claim-plan.json").exists() else None
            self.report["status"] = "complete"
        except Exception as error:
            self.report["status"] = "stopped"
            self.report["stop_reason"] = str(error)
        finally:
            for name in ("sql-templates", "scan-concurrency"):
                path = self.root/(name+".json")
                if path.exists():
                    self.report[name.replace("-", "_")] = read_json(path)
            self.report["elapsed_seconds"] = time.monotonic()-self.started
            self.report["peak_server_rss_mib"] = self.peak_rss_kib/1024 if self.peak_rss_kib else None
            self.report["peak_fixture_disk_mib"] = self.peak_disk_kib/1024 if self.peak_disk_kib else None
            self.finished.set()
            output = Path(self.args.report)
            output.parent.mkdir(parents=True,exist_ok=True)
            output.write_text(json.dumps(self.report,indent=2)+"\n")
            if not self.args.keep_server:
                self.server.terminate()
                try: self.server.wait(timeout=10)
                except subprocess.TimeoutExpired: self.server.kill(); self.server.wait()
            print(json.dumps({"status":self.report["status"],"report":str(output),"elapsed_seconds":self.report["elapsed_seconds"]}))
        return self.report["status"] == "complete"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True)
    parser.add_argument("--items", type=int, default=274, choices=[8,32,128,274,512,2740,27400,82200])
    parser.add_argument("--concurrency", type=int, nargs="+", default=[1,5,20], choices=[1,5,20])
    parser.add_argument("--scan-concurrency", default="auto", choices=["auto", "1", "2", "4", "8"])
    parser.add_argument("--idle-seconds", type=int, choices=[0, 15], default=15, help="Use 0 to omit idle sampling in sequential scan comparisons")
    parser.add_argument("--max-seconds", type=int, default=600)
    parser.add_argument("--max-disk-mib", type=int, default=2048)
    parser.add_argument("--max-rss-mib", type=int, default=2048)
    parser.add_argument("--server-binary", default="target/debug/examples/capacity_server")
    parser.add_argument("--report", required=True)
    parser.add_argument("--media-file", help="Optional bounded local MP4 fixture, copied only into the new temporary source tree")
    parser.add_argument("--keep-server", action="store_true", help="Leave the fixture server alive until its bounded lifetime expires for browser verification")
    args = parser.parse_args()
    if not 30 <= args.max_seconds <= 3600 or not 128 <= args.max_rss_mib <= 4096 or not 128 <= args.max_disk_mib <= 4096:
        parser.error("time or memory budget outside allowed range")
    raise SystemExit(0 if Run(args).execute() else 1)


if __name__ == "__main__":
    main()
