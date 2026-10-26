"""Private real-chart parity/API checks. Prints counts, never chart geometry."""
import argparse
import collections
import concurrent.futures
import gzip
import json
from pathlib import Path
import struct
import sys
import urllib.error
import urllib.parse
import urllib.request

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import bluechart


class GmpReference(bluechart.Img):
    """Test-only adapter: embedded headers, GMP-relative data pointers."""
    def __init__(self, path):
        super().__init__(path)
        self.embedded = {}
        for part in list(self.parts.values()):
            if part.kind != 'GMP':
                continue
            header = super().read_part(part, 0, 64)
            for kind, at in [('TRE', 0x19), ('RGN', 0x1d), ('LBL', 0x21)]:
                base = struct.unpack_from('<I', header, at)[0]
                if base:
                    virtual = bluechart.Part(part.name, kind, part.size, part.blocks)
                    self.parts[part.name, kind] = virtual
                    self.embedded[id(virtual)] = (part, base)

    def read_part(self, part, offset=0, length=None):
        if id(part) in getattr(self, 'embedded', {}):
            parent, base = self.embedded[id(part)]
            return super().read_part(parent, base if offset == 0 else offset, length)
        return super().read_part(part, offset, length)


def normalized(features):
    fields = ('kind', 'type', 'coords', 'label', 'depth_m', 'symbol_detail', 'tile')
    return collections.Counter(json.dumps({k: float(f[k]) if k == 'depth_m' else f[k] for k in fields if k in f},
                              sort_keys=True, separators=(',', ':')) for f in features)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--url', default='http://127.0.0.1:8765')
    parser.add_argument('--charts-root', type=Path, required=True)
    parser.add_argument('--fixtures', type=Path, required=True,
                        help='Private JSON fixture manifest; never commit real-chart manifests')
    args = parser.parse_args()
    manifest = json.loads(args.fixtures.read_text())

    def get(path, expected=200, headers=None):
        statuses = (expected,) if isinstance(expected, int) else expected
        request = urllib.request.Request(args.url + path, headers=headers or {})
        try:
            response = urllib.request.urlopen(request, timeout=90)
        except urllib.error.HTTPError as error:
            assert error.code in statuses, (error.code, expected)
            return {'status': error.code}
        assert response.status in statuses
        data = response.read()
        if response.headers.get('Content-Encoding') == 'gzip':
            data = gzip.decompress(data)
        return json.loads(data)

    catalog = get('/api/charts')
    by_name = {entry['name']: entry for entry in catalog['charts']}
    reports = []
    bluechart.Img = GmpReference
    first_query = None
    for fixture in manifest['fixtures']:
        entry = by_name[fixture['name']]
        chart = bluechart.Chart(args.charts_root / fixture['path'], args.charts_root / 'artifacts/no-overview.sqlite3')
        try:
            for sample in fixture['samples']:
                bbox, bits = sample['bbox'], sample.get('bits', 22)
                reference = chart.features(bbox, bits)
                query = '/api/features?' + urllib.parse.urlencode({'chart': entry['id'], 'bbox': ','.join(map(str, bbox)), 'bits': bits})
                first_query = first_query or query
                actual = get(query, headers={'Accept-Encoding': 'gzip'})
                assert actual['warnings'] == reference['warnings'] == []
                assert actual['features'], 'Fixture must select a populated area'
                left, right = normalized(reference['features']), normalized(actual['features'])
                assert left == right, ('parity mismatch', sum((left-right).values()), sum((right-left).values()))
                reports.append({'features': len(actual['features']), 'soundings': sum('depth_m' in f for f in actual['features'])})
        finally:
            chart.close()
    assert first_query, 'At least one fixture is required'
    for query in ['bbox=nan,0,1,1', 'bbox=2,1,0,2', 'bbox=0,0,1,1&bits=99', 'bbox=0,0,1,1&fishing=2']:
        get('/api/features?' + query, 400)
    get('/api/metadata?chart=../../etc/passwd', 400)
    get('/api/metadata', (403, 404), {'Host': 'evil.example'})
    for path in ['/bluechart.py', '/charts/chart.img', '/../Cargo.toml']:
        get(path, 404)
    query = first_query + '&client=integration&generation='
    get(query + '20')
    get(query + '19', 409)
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as executor:
        pair = list(executor.map(lambda i: get(first_query + f'&client=parallel{i}&generation=1'), range(2)))
    assert all(r['features'] for r in pair)
    for case in manifest.get('auto_cases', []):
        query = urllib.parse.urlencode({'chart': 'auto', 'bbox': ','.join(map(str, case['bbox'])), 'bits': case.get('bits',22)})
        assert get('/api/features?' + query)['chart_id'] == by_name[case['expected']]['id']
    for case in manifest.get('overlay_cases', []):
        base, overlay = by_name[case['base']]['id'], by_name[case['overlay']]['id']
        sample = '&' + urllib.parse.urlencode({'bbox': ','.join(map(str, case['bbox'])), 'bits': case.get('bits',22)})
        a = get('/api/features?chart=' + base + sample)
        b = get('/api/features?chart=' + overlay + sample)
        combined = get('/api/features?chart=' + base + '&overlays=' + overlay + sample)
        assert combined['warnings'] == []
        assert len(combined['features']) == len(a['features']) + len(b['features'])
        assert combined['stats']['subdivisions'] <= 180
    for name in manifest.get('overview_charts', []):
        entry = by_name[name]
        metadata = get('/api/metadata?chart=' + entry['id'])
        assert metadata['overview_available']
        query = urllib.parse.urlencode({'chart': entry['id'], 'bbox': ','.join(map(str,metadata['coverage_bbox'])), 'bits':17})
        overview = get('/api/features?' + query)
        assert overview['features'] and overview['warnings'] == [] and overview['stats']['overview_cache']
    print(json.dumps({'status':'passed','samples':reports,'api_checks':'passed','concurrent_clients':2}, indent=2))


if __name__ == '__main__':
    main()
