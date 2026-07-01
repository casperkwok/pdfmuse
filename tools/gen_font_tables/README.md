# gen_font_tables

Regenerates the static font tables in `crates/pdfmuse-core/src/pdf/tables/`
(AGL glyph names, base encodings, Core-14 metrics). Run only when updating the
source data; the generated `.rs` files are committed so builds need no network.

## Sources
- Adobe Glyph List: https://raw.githubusercontent.com/adobe-type-tools/agl-aglfn/master/glyphlist.txt
- Base encodings:   https://raw.githubusercontent.com/mozilla/pdf.js/master/src/core/encodings.js
- Core-14 metrics:  https://raw.githubusercontent.com/mozilla/pdf.js/master/src/core/metrics.js

## Usage
Download the three files next to `gen.py`, then:

    python3 gen.py ../../crates/pdfmuse-core/src/pdf/tables
