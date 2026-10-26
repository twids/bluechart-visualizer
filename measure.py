"""Reproducible local chart checks; emits small metrics, never chart geometry."""
import collections
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import time

from bluechart import Chart


def memory():
    if os.name == 'nt':
        class Counters(ctypes.Structure):
            _fields_=[('cb',wintypes.DWORD),('PageFaultCount',wintypes.DWORD)]+[
                (name,ctypes.c_size_t) for name in ('PeakWorkingSetSize','WorkingSetSize',
                'QuotaPeakPagedPoolUsage','QuotaPagedPoolUsage','QuotaPeakNonPagedPoolUsage',
                'QuotaNonPagedPoolUsage','PagefileUsage','PeakPagefileUsage')]
        counters=Counters();counters.cb=ctypes.sizeof(counters)
        kernel=ctypes.WinDLL('kernel32');kernel.GetCurrentProcess.restype=wintypes.HANDLE
        psapi=ctypes.WinDLL('psapi')
        psapi.GetProcessMemoryInfo.argtypes=[wintypes.HANDLE,ctypes.POINTER(Counters),wintypes.DWORD]
        if not psapi.GetProcessMemoryInfo(kernel.GetCurrentProcess(),ctypes.byref(counters),counters.cb):
            raise ctypes.WinError()
        return {'working_set_bytes':counters.WorkingSetSize,'peak_working_set_bytes':counters.PeakWorkingSetSize}
    import resource
    return {'peak_working_set_bytes':resource.getrusage(resource.RUSAGE_SELF).ru_maxrss*1024}


def main():
    start=time.perf_counter();baseline=memory();chart=Chart()
    try:
        startup={'seconds':time.perf_counter()-start,'bytes_read':chart.img.bytes_read,**memory()}
        result=chart.features([18.68,59.23,18.81,59.30],24)
        cold={**result['stats'],**memory()}
        reference=[18+42.249/60,59+17.134/60]
        candidates=[f for f in result['features'] if f['kind']=='point' and f['label']=='Stavsnäs']
        point=min(candidates,key=lambda f:sum((a-b)**2 for a,b in zip(f['coords'][0],reference)))
        soundings=[f['depth_m'] for f in result['features'] if 'depth_m' in f]
        report={'file_bytes':chart.img.size,'title':chart.img.title,'baseline':baseline,
                'startup':startup,'first_view':cold,'repeat_view':chart.features([18.68,59.23,18.81,59.30],24)['stats'],
                'tile_count':len(chart.tiles),'subdivision_count':sum(sum(l['count'] for l in t['levels']) for t in chart.tiles),
                'soundings':{'count':len(soundings),'min_m':min(soundings),'max_m':max(soundings)},
                'type_counts':dict(collections.Counter(f"{f['kind']}:0x{f['type']:x}" for f in result['features'])),
                'warnings':result['warnings'],
                'geographic_check':{'reference_lonlat':reference,'chart_lonlat':point['coords'][0],
                'reference_url':'https://www.varmdo.se/varmdohamnar/stavsnasvinterhamn.4.1524f1c618a8d1d9fee45c6d.html',
                'scope':'Approximate location check only; not a navigation accuracy assessment.'}}
        out=Path(__file__).parent/'artifacts';out.mkdir(exist_ok=True)
        (out/'measurements.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')
        print(json.dumps({k:v for k,v in report.items() if k not in ('type_counts',)},ensure_ascii=True,indent=2))
    finally:chart.img.close()


if __name__=='__main__':main()
