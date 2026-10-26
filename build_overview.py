"""Build a private local overview cache. Never add its output to Git."""
import argparse
import json
import os
import sqlite3
from pathlib import Path
import time
from bluechart import Chart, DEFAULT_CHART, ROOT


def simplify(points,tolerance):
    if len(points)<4:
        return points
    closed=points+[points[0]] if points[-1]!=points[0] else points
    keep={0,len(closed)-1};stack=[(0,len(closed)-1)];limit=tolerance*tolerance
    while stack:
        a,b=stack.pop();ax,ay=closed[a];bx,by=closed[b];dx=bx-ax;dy=by-ay;length=dx*dx+dy*dy
        farthest,index=limit,None
        for i in range(a+1,b):
            x,y=closed[i];t=max(0,min(1,((x-ax)*dx+(y-ay)*dy)/length)) if length else 0
            distance=(x-ax-t*dx)**2+(y-ay-t*dy)**2
            if distance>farthest:farthest,index=distance,i
        if index is not None:
            keep.add(index);stack.extend(((a,index),(index,b)))
    result=[closed[i] for i in sorted(keep)][:-1]
    return result if len(result)>=3 else []


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--chart',type=Path,default=DEFAULT_CHART)
    parser.add_argument('--output',type=Path,default=ROOT/'.cache'/'overview.sqlite3')
    args=parser.parse_args();start=time.perf_counter();chart=Chart(args.chart)
    args.output.parent.mkdir(parents=True,exist_ok=True)
    temp=args.output.with_name(args.output.name+'.tmp')
    if temp.exists():
        raise RuntimeError(f'Incomplete cache already exists: {temp}; inspect it before retrying')
    db=sqlite3.connect(temp)
    db.executescript('CREATE TABLE metadata(payload TEXT); CREATE TABLE overview(id INTEGER PRIMARY KEY, level INTEGER, payload TEXT); CREATE INDEX level_idx ON overview(level); CREATE VIRTUAL TABLE bounds USING rtree(id,west,east,south,north);')
    levels={13:0,15:0,17:0};warnings=[];count=0
    tiles=[t for t in chart.tiles if t['id'].startswith('M')]
    for ti,tile in enumerate(tiles):
        chart.load_tile(tile)
        bits=min(l['bits'] for l in tile['levels'] if not l['inherited'])
        for sub in tile['subs']:
            if sub['bits']!=bits:continue
            payload=chart.decode_sub(tile,sub);warnings.extend(payload['warnings']);count+=1
            for f in payload['features']:
                if f['kind']!='polygon' or f['type']!=0x10101:continue
                for level,tolerance in [(13,.04),(15,.01),(17,.002)]:
                    box=f['bbox']
                    if max(box[2]-box[0],box[3]-box[1])<tolerance:continue
                    points=simplify(f['coords'],tolerance)
                    if points:
                        feature={'kind':'polygon','type':f['type'],'label':'','tile':tile['id'],
                                 'bbox':box,'coords':[[round(x,6),round(y,6)] for x,y in points]}
                        cursor=db.execute('INSERT INTO overview(level,payload) VALUES (?,?)',(level,json.dumps(feature,separators=(',',':'))))
                        db.execute('INSERT INTO bounds VALUES (?,?,?,?,?)',(cursor.lastrowid,box[0],box[2],box[1],box[3]))
                        levels[level]+=1
        chart.cache.clear();chart.cache_bytes=0
        print(f'{ti+1}/{len(tiles)} tiles; {count} subdivisions; {time.perf_counter()-start:.1f}s',flush=True)
    manifest={'format':1,'size':chart.img.size,'mtime':int(chart.img.path.stat().st_mtime),'title':chart.img.title}
    chart.img.close()
    if warnings:
        db.close()
        raise RuntimeError(f'Overview not published: {len(warnings)} decoding errors; first: {warnings[0]}')
    db.execute('INSERT INTO metadata VALUES (?)',(json.dumps(manifest),))
    db.commit()
    if db.execute('PRAGMA integrity_check').fetchone()[0]!='ok':
        db.close()
        raise RuntimeError('Overview SQLite integrity check failed')
    db.close()
    os.replace(temp,args.output)
    print(json.dumps({'bytes':args.output.stat().st_size,'features_per_level':levels,'seconds':round(time.perf_counter()-start,2)}),flush=True)


if __name__=='__main__':main()
