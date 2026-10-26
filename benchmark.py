"""Read-only performance benchmark. Prints metrics/digests, never map geometry."""
import argparse
import gzip
import hashlib
import json
import time
from bluechart import Chart, DEFAULT_CHART


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--chart',default=DEFAULT_CHART)
    args=parser.parse_args()
    bbox=[18.60,59.226106892723635,18.90,59.30388467257629]
    for fishing in (False,True):
        chart=Chart(args.chart)
        try:
            for repeat in range(2):
                payload=chart.features(bbox,22,include_fishing=fishing)
                digest=hashlib.sha256(json.dumps(payload['features'],sort_keys=True).encode()).hexdigest()
                started=time.perf_counter()
                compact=[{**{k:f[k] for k in ('kind','type','label','tile','depth_m','symbol_detail') if k in f},
                          'coords':[[round(x,6),round(y,6)] for x,y in f['coords']]}
                         for f in payload['features']]
                wire=json.dumps(compact,separators=(',',':'),ensure_ascii=False).encode()
                compressed=gzip.compress(wire,compresslevel=1,mtime=0)
                print(json.dumps({'fishing':fishing,'repeat':repeat,**payload['stats'],
                      'warnings':len(payload['warnings']),'geometry_sha256':digest,
                      'compact_bytes':len(wire),'gzip_bytes':len(compressed),
                      'wire_seconds':round(time.perf_counter()-started,3)}),flush=True)
        finally:chart.img.close()


if __name__=='__main__':main()
