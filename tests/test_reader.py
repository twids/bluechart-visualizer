"""Synthetic fixtures contain no real chart data."""
import json
import gzip
import os
from pathlib import Path
import struct
import sys
import tempfile
import threading
import unittest
import urllib.error
import urllib.request
from http.server import ThreadingHTTPServer

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from bluechart import (Img, Chart, FormatError, Bits, UNIT, decode_poly,
                       decode_point, deltas, sint, uint, variable_uint)
from server import handler_for
from build_overview import simplify


def fixture(path):
    data = bytearray(14*512)
    data[16:23] = b'DSKIMG\0'
    data[65:72] = b'GARMIN\0'
    data[0x61] = 9
    data[0x49:0x5d] = b'TEST'.ljust(20,b' ')
    data[0x65:0x83] = b' '.ljust(30,b' ')
    data[0x600] = 1
    data[0x601:0x60c] = b' '*11
    struct.pack_into('<I',data,0x60c,0xc00)
    for offset,part,block in [(0x800,0,10),(0xa00,1,12)]:
        data[offset] = 1
        data[offset+1:offset+12] = b'MAP00001RGN'
        struct.pack_into('<I',data,offset+12,800 if not part else 0)
        data[offset+17] = part
        struct.pack_into('<240H',data,offset+32,block,*([65535]*239))
    data[10*512:11*512] = b'A'*512
    data[12*512:13*512] = b'B'*512
    path.write_bytes(data)
    return data


class ReaderTests(unittest.TestCase):
    def test_fragmented_multipart_random_access(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'test.img'; fixture(p)
            img=Img(p)
            try:
                part=img.parts['MAP00001','RGN']
                self.assertEqual(part.blocks,[10,12])
                self.assertEqual(img.read_part(part,500,25),b'A'*12+b'B'*13)
                self.assertEqual(img.read_part(part,790,10),b'B'*10)
                self.assertLessEqual(img.max_read,512)
                for offset,length in [(-1,1),(799,2),(0,801)]:
                    with self.assertRaises(FormatError):img.read_part(part,offset,length)
            finally:img.close()

    def test_invalid_signature_and_missing_continuation(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'test.img';data=fixture(p)
            data[0xa11]=2;p.write_bytes(data)
            with self.assertRaisesRegex(FormatError,'continuation'):Img(p)
            data[16]=0;p.write_bytes(data)
            with self.assertRaisesRegex(FormatError,'signature'):Img(p)

    def test_signed_coordinates_and_truncated_fields(self):
        self.assertEqual(sint(bytes.fromhex('ffffff'),0),-1)
        self.assertEqual(sint(bytes.fromhex('000080'),0),-(1<<23))
        self.assertEqual(sint(bytes.fromhex('ffff7f'),0),(1<<23)-1)
        with self.assertRaises(FormatError):uint(b'\0',0,3)
        with self.assertRaises(FormatError):Bits(b'\0').get(9)

    def test_polygon_start_and_packed_delta(self):
        sub={'cx':1000,'cy':2000,'shift':0}
        data=b'\x01\0\0\0'+struct.pack('<hh',-2,3)+b'\x01\x00\x65'
        feature,end=decode_poly(data,0,sub)
        self.assertEqual(end,len(data))
        self.assertEqual(feature['coords'],[[998*UNIT,2003*UNIT],[1000*UNIT,2004*UNIT]])
        with self.assertRaises(FormatError):decode_poly(data[:-1],0,sub)

    def test_variable_signed_delta_extension(self):
        # x/y variable signs; x is a sign-extension sentinel then +2, y is -1.
        values=[(0,1),(0,1),(4,3),(2,3),(7,3)]
        number=0;shift=0
        for val,bits in values:number|=val<<shift;shift+=bits
        self.assertEqual(list(deltas(number.to_bytes(2,'little'),0)),[[5,-1]])

    def test_extended_sounding_class_attribute(self):
        sub={'cx':1000,'cy':2000,'shift':0}
        # 0x10301 depth point, class field 0x88: one byte, tenths of a metre.
        data=bytes([3,0x81])+struct.pack('<hh',-10,20)+bytes([0x88,84])
        feature,end=decode_point(data,0,sub,True)
        self.assertEqual(feature['type'],0x10301)
        self.assertEqual(feature['depth_m'],8.4)
        self.assertEqual(feature['coords'][0],[990*UNIT,2020*UNIT])
        self.assertEqual(end,len(data))
        with self.assertRaises(FormatError):decode_point(data[:-1],0,sub,True)

    def test_variable_lengths(self):
        self.assertEqual(variable_uint(bytes([21]),0),(10,1))
        self.assertEqual(variable_uint(struct.pack('<H',(500<<2)|2),0),(500,2))
        with self.assertRaises(FormatError):variable_uint(b'\0',0)

    def test_overview_simplification_preserves_large_outline(self):
        ring=[[0,0],[.25,0],[.5,0],[1,0],[1,1],[.5,1],[0,1]]
        simplified=simplify(ring,.01)
        self.assertEqual(set(map(tuple,simplified)),{(0,0),(1,0),(1,1),(0,1)})

    def test_buffered_bits_match_reference_across_word_boundaries(self):
        data=bytes(range(256));reader=Bits(data);value=int.from_bytes(data,'little');pos=0
        while reader.remaining:
            count=min(1+(pos%23),reader.remaining)
            self.assertEqual(reader.get(count),(value>>pos)&((1<<count)-1))
            pos+=count
        self.assertEqual(reader.remaining,0)
        with self.assertRaises(FormatError):reader.get(1)


class HttpTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        class Dummy:
            def metadata(self):return {'title':'Synthetic'}
            def features(self,bbox,bits,**kwargs):
                return {'features':[{'kind':'line','type':1,'coords':[[18.700000023,59.250000013]]*1200,
                         'label':'test','tile':'M0000001','label_ref':123,'bbox':[18.7,59.25,18.7,59.25]}],
                        'warnings':[],'stats':{}}
        cls.server=ThreadingHTTPServer(('127.0.0.1',0),handler_for(Dummy(),['viewer.example.test']))
        cls.thread=threading.Thread(target=cls.server.serve_forever,daemon=True);cls.thread.start()
        cls.url=f'http://127.0.0.1:{cls.server.server_port}'

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown();cls.server.server_close();cls.thread.join()

    def test_local_metadata_and_bad_inputs(self):
        with urllib.request.urlopen(self.url+'/api/metadata') as response:
            self.assertEqual(json.load(response)['title'],'Synthetic')
        for query in ['nan,59,19,60','18,60,17,59','1,2,3','18,59,19,60&bits=99']:
            with self.assertRaises(urllib.error.HTTPError) as error:
                urllib.request.urlopen(self.url+'/api/features?bbox='+query)
            self.assertEqual(error.exception.code,400)

    def test_only_explicit_assets_and_local_host(self):
        for path in ['/bluechart.py','/charts/test.img','/../bluechart.py']:
            with self.assertRaises(urllib.error.HTTPError) as error:urllib.request.urlopen(self.url+path)
            self.assertEqual(error.exception.code,404)
        request=urllib.request.Request(self.url+'/api/metadata',headers={'Host':'external.example'})
        with self.assertRaises(urllib.error.HTTPError) as error:urllib.request.urlopen(request)
        self.assertEqual(error.exception.code,403)

    def test_explicit_proxy_host(self):
        request=urllib.request.Request(self.url+'/api/metadata',headers={'Host':'viewer.example.test'})
        with urllib.request.urlopen(request) as response:
            self.assertEqual(json.load(response)['title'],'Synthetic')
        request=urllib.request.Request(self.url+'/api/metadata',headers={'Host':'viewer.example.test.evil.example'})
        with self.assertRaises(urllib.error.HTTPError) as error:urllib.request.urlopen(request)
        self.assertEqual(error.exception.code,403)

    def test_compact_gzip_preserves_display_fields(self):
        request=urllib.request.Request(self.url+'/api/features?bbox=18.68,59.23,18.81,59.30&compact=1&fishing=0',headers={'Accept-Encoding':'gzip'})
        with urllib.request.urlopen(request) as response:
            self.assertEqual(response.headers['Content-Encoding'],'gzip')
            data=json.loads(gzip.decompress(response.read()))
        f=data['features'][0]
        self.assertEqual(f['coords'][0],[18.7,59.25])
        self.assertEqual(f['label'],'test')
        self.assertNotIn('label_ref',f)
        self.assertNotIn('bbox',f)

    def test_requests_outside_old_demo_box_are_accepted(self):
        with urllib.request.urlopen(self.url+'/api/features?bbox=5.25,60.33,5.42,60.43&bits=22') as response:
            self.assertTrue(json.load(response)['features'])


@unittest.skipUnless(os.environ.get('BLUECHART_INTEGRATION')=='1',
                         'Set BLUECHART_INTEGRATION=1 for local chart validation')
class RealChartTests(unittest.TestCase):
    def open_chart(self):
        path=os.environ.get('BLUECHART_CHART')
        bbox=os.environ.get('BLUECHART_TEST_BBOX')
        if not path or not bbox:
            self.fail('Set BLUECHART_CHART and BLUECHART_TEST_BBOX for integration checks')
        return Chart(Path(path)),[float(v) for v in bbox.split(',')]

    def test_user_chart_and_overview(self):
        chart,bbox=self.open_chart()
        try:
            self.assertTrue(chart.tiles)
            result=chart.features(bbox,22,include_fishing=False)
            self.assertTrue(result['features'],'Choose a populated test bounding box')
            self.assertEqual(result['warnings'],[])
            self.assertLessEqual(result['stats']['subdivisions'],180)
            if chart.overview:
                result=chart.features(chart.metadata()['coverage_bbox'],24,include_fishing=False)
                self.assertEqual(result['warnings'],[])
                self.assertTrue(result['features'])
                self.assertLessEqual(result['stats']['effective_bits'],17)
        finally:chart.close()

    def test_user_chart_lazy_reads_and_repeatability(self):
        chart,bbox=self.open_chart()
        try:
            self.assertEqual(chart.warnings,[])
            self.assertTrue(all(t['subs'] is None for t in chart.tiles))
            result=chart.features(bbox)
            self.assertEqual(result['warnings'],[])
            self.assertTrue(result['features'],'Choose a populated test bounding box')
            cached=chart.features(bbox)
            self.assertEqual(cached['features'],result['features'])
            self.assertLessEqual(chart.cache_bytes,chart.cache_limit)
            self.assertLessEqual(chart.img.max_read,chart.img.block_size)
            filtered=chart.features(bbox,include_fishing=False)
            self.assertEqual(filtered['features'],[f for f in result['features'] if not f['tile'].startswith('F')])
            with self.assertRaises(InterruptedError):
                chart.features(bbox,cancelled=lambda:True)
        finally:chart.close()


if __name__=='__main__':unittest.main()
