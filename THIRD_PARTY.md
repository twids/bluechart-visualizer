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

No external JavaScript, fonts, image assets or online map tiles are used by the viewer.
