"""Local-only BlueChart HTTP viewer. SPDX-License-Identifier: GPL-3.0-only."""
import argparse
import collections
import gzip
import json
import math
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlsplit

from bluechart import Chart, DEFAULT_CHART, FormatError, ROOT, intersects

TEST_BOUNDS = [18.60,59.18,18.90,59.35]


def handler_for(chart, allowed_hosts=()):
    latest = collections.OrderedDict()
    request_lock = threading.Lock()
    class Handler(BaseHTTPRequestHandler):
        def send(self, code, data, content_type='application/json; charset=utf-8'):
            if not isinstance(data,bytes):
                data = json.dumps(data,ensure_ascii=False,allow_nan=False,separators=(',',':')).encode('utf-8')
            compressed = len(data)>4096 and 'gzip' in self.headers.get('Accept-Encoding','')
            if compressed:
                data = gzip.compress(data,compresslevel=1,mtime=0)
            self.send_response(code)
            self.send_header('Content-Type',content_type)
            self.send_header('Content-Length',str(len(data)))
            self.send_header('Cache-Control','no-store')
            self.send_header('Vary','Accept-Encoding')
            if compressed:
                self.send_header('Content-Encoding','gzip')
            self.send_header('X-Content-Type-Options','nosniff')
            self.send_header('Content-Security-Policy',"default-src 'self'; style-src 'self'; script-src 'self'; connect-src 'self'; img-src 'self' data:; frame-ancestors 'none'")
            self.end_headers()
            self.wfile.write(data)

        def do_GET(self):
            # Restrict Host as well as bind address (including DNS rebinding).
            host = self.headers.get('Host','')
            if host not in {f'127.0.0.1:{self.server.server_port}',f'localhost:{self.server.server_port}',*allowed_hosts}:
                self.send(403,{'error':'Host not allowed'})
                return
            url = urlsplit(self.path)
            try:
                if url.path == '/api/metadata':
                    metadata = chart.metadata()
                    metadata['test_bounds'] = TEST_BOUNDS
                    self.send(200,metadata)
                elif url.path == '/api/features':
                    args = parse_qs(url.query)
                    bbox = [float(v) for v in args.get('bbox',[''])[0].split(',')]
                    bits = int(args.get('bits',['24'])[0])
                    if (len(bbox) != 4 or not all(math.isfinite(v) for v in bbox)
                        or not -180 <= bbox[0] < bbox[2] <= 180
                        or not -85 <= bbox[1] < bbox[3] <= 85 or not 17 <= bits <= 24):
                        raise ValueError('Invalid bbox or detail level')
                    if not intersects(bbox,TEST_BOUNDS):
                        self.send(200,{'features':[],'warnings':['Utanför testområdet. Återställ utsnittet.'],'stats':{}})
                        return
                    clipped = [max(bbox[0],TEST_BOUNDS[0]),max(bbox[1],TEST_BOUNDS[1]),
                               min(bbox[2],TEST_BOUNDS[2]),min(bbox[3],TEST_BOUNDS[3])]
                    client = args.get('client',[''])[0]
                    generation = int(args.get('generation',['0'])[0])
                    if len(client)>64 or generation<0:
                        raise ValueError('Invalid request generation')
                    if client:
                        with request_lock:
                            latest[client] = max(generation,latest.get(client,-1))
                            latest.move_to_end(client)
                            while len(latest)>256:
                                latest.popitem(last=False)
                    cancelled = (lambda: latest.get(client) != generation) if client else None
                    fishing = args.get('fishing',['1'])[0]
                    if fishing not in ('0','1'):
                        raise ValueError('Invalid fishing layer')
                    payload = chart.features(clipped,bits,include_fishing=fishing=='1',cancelled=cancelled)
                    if args.get('compact',['0'])[0]=='1':
                        # Omit decoder bookkeeping, quantize only wire coordinates (<0.06 m).
                        payload['features'] = [
                            {**{k:f[k] for k in ('kind','type','label','tile','depth_m','symbol_detail') if k in f},
                             'coords':[[round(x,6),round(y,6)] for x,y in f['coords']]}
                            for f in payload['features']]
                    self.send(200,payload)
                elif url.path in ('/','/app.js','/style.css'):
                    filename,content_type = {
                        '/':('index.html','text/html; charset=utf-8'),
                        '/app.js':('app.js','text/javascript; charset=utf-8'),
                        '/style.css':('style.css','text/css; charset=utf-8')}[url.path]
                    self.send(200,(ROOT/'web'/filename).read_bytes(),content_type)
                else:
                    self.send(404,{'error':'Not found'})
            except (ValueError,FormatError) as exc:
                self.send(400,{'error':str(exc)})
            except InterruptedError:
                self.send(409,{'error':'Superseded map request'})
            except (BrokenPipeError,ConnectionResetError):
                pass

        def log_message(self,format,*args):
            print(format % args,flush=True)
    return Handler


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--chart',type=Path,default=DEFAULT_CHART)
    parser.add_argument('--port',type=int,default=8765)
    parser.add_argument('--bind',default='127.0.0.1',help='Bind address; container deployments use 0.0.0.0')
    parser.add_argument('--allowed-host',action='append',default=[],help='Additional exact Host header accepted behind the proxy')
    args = parser.parse_args()
    chart = Chart(args.chart)
    try:
        with ThreadingHTTPServer((args.bind,args.port),handler_for(chart,args.allowed_host)) as server:
            server.daemon_threads = True
            print(f'BlueChart: http://127.0.0.1:{server.server_port} ({len(chart.tiles)} tiles)',flush=True)
            server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        chart.img.close()


if __name__ == '__main__':
    main()
