#!/usr/bin/env python3
"""Build a tiny valid EPUB fixture for the server's EPUB reader.

Usage: python3 tools/make-test-epub.py [output.epub]

Three chapters, a navigation document, one style sheet. The word
"vesper" appears in chapter two so search has something to find.
"""
import sys
import zipfile

OUT = sys.argv[1] if len(sys.argv) > 1 else "test.epub"

CONTAINER = """<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>
"""

OPF = """<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="bookid" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="bookid">urn:umiak:test-epub</dc:identifier>
    <dc:title>The Test Voyage</dc:title>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="ch2.xhtml" media-type="application/xhtml+xml"/>
    <item id="c3" href="ch3.xhtml" media-type="application/xhtml+xml"/>
    <item id="css" href="style.css" media-type="text/css"/>
  </manifest>
  <spine>
    <itemref idref="c1"/>
    <itemref idref="c2"/>
    <itemref idref="c3"/>
  </spine>
</package>
"""

NAV = """<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head><title>Contents</title></head>
<body>
<nav epub:type="toc">
<ol>
<li><a href="ch1.xhtml">Leaving Shore</a></li>
<li><a href="ch2.xhtml">The Open Water</a></li>
<li><a href="ch3.xhtml">Harbour</a></li>
</ol>
</nav>
</body>
</html>
"""

CH1 = """<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Leaving Shore</title><link rel="stylesheet" href="style.css"/></head>
<body><h1>Leaving Shore</h1><p>The boat carries the whole household.</p></body>
</html>
"""

CH2 = """<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>The Open Water</title><link rel="stylesheet" href="style.css"/></head>
<body><h1>The Open Water</h1><p>By vesper light the crew reads the stars.</p></body>
</html>
"""

CH3 = """<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Harbour</title><link rel="stylesheet" href="style.css"/></head>
<body><h1>Harbour</h1><p>Everything aboard is accounted for.</p></body>
</html>
"""

CSS = "body { font-family: serif; margin: 2em; }\n"

with zipfile.ZipFile(OUT, "w") as z:
    # The mimetype entry must be first and stored, per the EPUB spec.
    z.writestr(zipfile.ZipInfo("mimetype"), "application/epub+zip",
               compress_type=zipfile.ZIP_STORED)
    z.writestr("META-INF/container.xml", CONTAINER)
    z.writestr("OEBPS/content.opf", OPF)
    z.writestr("OEBPS/nav.xhtml", NAV)
    z.writestr("OEBPS/ch1.xhtml", CH1)
    z.writestr("OEBPS/ch2.xhtml", CH2)
    z.writestr("OEBPS/ch3.xhtml", CH3)
    z.writestr("OEBPS/style.css", CSS)

print("wrote", OUT)
