#!/usr/bin/env python3
"""Verify HTTP ranges, direct images and NFO updates in an owned capacity fixture only."""
import argparse
import concurrent.futures
import hashlib
import json
from pathlib import Path
import sqlite3
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from run import distribution


class Fixture:
    def __init__(self, root):
        self.root = Path(root).resolve()
        if (self.root / '.tjxy-capacity').read_text() != 'isolated-capacity-v1':
            raise ValueError('not an isolated capacity fixture')
        config = json.loads((self.root / 'connection.json').read_text())
        self.base = config['base_url']
        if urllib.parse.urlparse(self.base).hostname != '127.0.0.1':
            raise ValueError('fixture must use loopback')
        self.token = None
        status, _, data = self.request('/Users/AuthenticateByName', {'Username': config['username'], 'Pw': config['password']})
        assert status == 200
        auth = json.loads(data)
        self.token, self.user = auth['AccessToken'], auth['User']['Id']

    def request(self, path, body=None, method=None, headers=None):
        url = urllib.parse.urljoin(self.base, path)
        assert urllib.parse.urlparse(url).netloc == urllib.parse.urlparse(self.base).netloc
        supplied = {'Authorization': 'MediaBrowser Client="LocalVerification", Device="Local", DeviceId="local-verification", Version="1"'}
        if self.token:
            supplied['Authorization'] += ', Token="' + self.token + '"'
        if body is not None:
            supplied['Content-Type'] = 'application/json'
        supplied.update(headers or {})
        req = urllib.request.Request(url, data=None if body is None else json.dumps(body).encode(), headers=supplied, method=method)
        try:
            response = urllib.request.urlopen(req, timeout=15)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            return response.status, {k.lower(): v for k, v in response.headers.items()}, response.read(16 * 1024 * 1024)

    def rows(self, sql, args=()):
        with sqlite3.connect(self.root / 'catalog.db', timeout=10) as db:
            return db.execute(sql, args).fetchall()

    def task(self, path):
        status, _, data = self.request(path, {}, 'POST')
        assert status == 202, (path, status)
        job = uuid.UUID(json.loads(data)['JobId']).bytes
        deadline = time.monotonic() + 120
        while time.monotonic() < deadline:
            row = self.rows('SELECT state FROM work_jobs WHERE id=?', (job,))[0]
            if row[0] in ('Completed', 'Failed'):
                assert row[0] == 'Completed', (path, row[0])
                return
            time.sleep(.2)
        raise TimeoutError('local verification task exceeded 120 seconds')

    def verify(self, mutate):
        movies = self.rows("SELECT id,name,production_year FROM catalog_items WHERE item_type='Movie' ORDER BY name")
        assert movies
        item = str(uuid.UUID(bytes=movies[0][0]))
        directory = self.root / 'media' / (movies[0][1] + ' (2001)')
        video = next(directory.glob('*.mp4')).read_bytes()
        status, _, data = self.request('/Items/' + item + '/PlaybackInfo?userId=' + self.user, {'DeviceProfile': {'DirectPlayProfiles': [{'Type': 'Video', 'Container': 'mp4'}]}})
        assert status == 200
        source = json.loads(data)['MediaSources'][0]
        stream = source['DirectStreamUrl']
        stream_headers = source.get('RequiredHttpHeaders') or {}
        samples = []
        for concurrency in (1, 5, 20):
            def read_range(index):
                start = (index * 16384) % max(1, len(video) - 65536)
                end = min(start + 65535, len(video) - 1)
                before = time.monotonic()
                status, headers, data = self.request(stream, headers={**stream_headers, 'Range': f'bytes={start}-{end}'})
                elapsed = (time.monotonic() - before) * 1000
                assert status == 206 and data == video[start:end + 1]
                assert headers['content-range'] == f'bytes {start}-{end}/{len(video)}'
                return elapsed
            with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as pool:
                samples.append({'concurrency': concurrency, 'range_64k': distribution(list(pool.map(read_range, range(max(20, concurrency * 5)))))})
        status, headers, data = self.request(stream, method='HEAD', headers=stream_headers)
        assert status == 200 and not data and int(headers['content-length']) == len(video)
        status, headers, _ = self.request(stream, headers={**stream_headers, 'Range': f'bytes={len(video)}-'})
        assert status == 416 and headers['content-range'] == f'bytes */{len(video)}'
        poster = directory / 'poster.png'
        original = poster.read_bytes()
        image = '/Items/' + item + '/Images/Primary'
        status, headers, data = self.request(image)
        assert status == 200 and data == original
        etag = headers['etag']
        status, _, data = self.request(image, method='HEAD')
        assert status == 200 and not data
        status, _, data = self.request(image, headers={'If-None-Match': etag})
        assert status == 304 and not data
        result = {'movies': len(movies), 'range_latency': samples, 'video_bytes': len(video), 'video_sha256': hashlib.sha256(video).hexdigest(), 'range_bytes_exact': True, 'range_unsatisfiable_status': 416, 'head_without_body': True, 'image_condition_status': 304}
        if mutate:
            poster.unlink()
            try:
                status, _, _ = self.request(image)
                assert status == 404, status
                result['missing_poster_status'] = status
            finally:
                poster.write_bytes(original)
            status, _, data = self.request(image)
            result['restored_poster_before_rescan_status'] = status
            # Recreating a file can change its inode/provider identity. Reconcile
            # the source inventory and reference before requiring it to be served.
            root_id = str(uuid.UUID(bytes=self.rows('SELECT storage_root_id FROM library_storage_roots LIMIT 1')[0][0]))
            self.task('/Admin/Tasks/ValidateStorage/' + root_id)
            self.task('/Admin/Tasks/ResolveMetadata/' + item)
            status, _, data = self.request(image)
            assert status == 200 and data == original
            result['restored_poster_status'] = status
            nfo = directory / 'movie.nfo'
            xml = nfo.read_text()
            assert '<year>2001</year>' in xml
            nfo.write_text(xml.replace('<year>2001</year>', '<year>2003</year>'))
            self.task('/Admin/Tasks/ValidateStorage/' + root_id)
            self.task('/Admin/Tasks/ResolveMetadata/' + item)
            updated = self.rows("SELECT id,production_year FROM catalog_items WHERE item_type='Movie' ORDER BY name")
            assert updated[0][1] == 2003
            assert [row[1] for row in updated[1:]] == [row[2] for row in movies[1:]]
            result['nfo_updated_year'] = 2003
            result['other_movie_years_unchanged'] = True
        result['assets'] = {table: self.rows('SELECT COUNT(*) FROM ' + table)[0][0] for table in ('asset_blobs', 'item_assets', 'person_assets')}
        result['asset_files'] = sum(path.is_file() for path in (self.root / 'assets').rglob('*'))
        assert not any(result['assets'].values()) and result['asset_files'] == 0
        return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', required=True)
    parser.add_argument('--report', required=True)
    parser.add_argument('--mutate', action='store_true', help='Change only the owned fixture poster and first NFO')
    args = parser.parse_args()
    result = Fixture(args.root).verify(args.mutate)
    Path(args.report).write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))
