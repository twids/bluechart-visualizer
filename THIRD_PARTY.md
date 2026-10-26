# Parser references and licensing

This prototype is licensed under GPL-3.0-only; see LICENSE.
The map file is user supplied, is not part of the software, and retains its own rights.

The Rust coordinate bitstream and point/polygon decoding are adapted from:

- QMapShack, Copyright (C) 2006-2008 Oliver Eichler, GPL-3.0-or-later.
  https://github.com/Maproom/qmapshack/tree/dev/src/qmapshack/map/garmin
  Reference files: CGarminPolygon.cpp, CGarminPoint.cpp, CGarminStrTbl6.cpp,
  CMapIMG.cpp and CMapIMG.h.
- GPXSee, Copyright (C) Martin Tůma and contributors, GPL-3.0.
  https://github.com/tumic0/GPXSee/tree/master/src/map/IMG
  Reference files: trefile.cpp, rgnfile.cpp, subfile_img.cpp, vectortile_img.cpp,
  deltastream.cpp and style_img.cpp. Used for flag-dependent TRE7 fields,
  GMP embedded subfile offsets, variable-length integers, reserved coordinate bits,
  class/local/global attribute lengths, marine soundings and initial marine type styles.
  End-of-stream delta escapes follow DeltaStream::readNext/atEnd: retain complete
  coordinate pairs and ignore an incomplete trailing pair within the declared bytes.

No external JavaScript, fonts, image assets or online map tiles are used by the viewer.

## Marine feature catalog

`web/features.js` uses the marine point, line and polygon type definitions in
the cGPSmapper User Manual 2.4.5, sections 4.3 and 12.3 (a copy of the original
manual is available at
https://device.report/m/117ea67e6f4c646a3325832f5c3bd84391e86e7d082cca6c875d28c26d132ebf.pdf).
The IMG decoder represents marine types with a `0x10000` prefix. Type namespaces
are separate for each geometry kind. For example, `0x10409` is a marine farm
point, but a danger boundary/area for lines/polygons. `0x10301` is an intertidal
polygon, a depth contour line, or a sounding point.

GPXSee's `style_img.cpp` is also a reference for marine styles and extended
background types. The local sample inventory establishes contour usage for
line extensions `0x10308` and `0x10309` and depth-area usage for polygon
`0x10308`; numeric depth ranges are not inferred from these codes.
Other unverified extensions (including `0x10802` points, `0x10614` features and
`0x10d03` areas) remain explicitly unknown. The viewer suppresses `0x10614`
because it appears repeatedly as long, unwanted geometry in the supplied chart
models; this does not establish the code's intended meaning. Unknown areas such
as `0x10d03` are available through the optional **Unidentified areas** layer,
which is off by default.
Some public polygon definitions specify invisible areas; the viewer now draws
simplified outlines and a light tint to make their geometry inspectable.
No proprietary Garmin symbol images are bundled; marker drawings are simple
canvas geometry and do not reproduce the full Garmin renderer.
