// Print the raw IR JSON for a file via the WASM (wasm-bindgen) binding.
// Used by the cross-binding parity gate (tests/parity/check.py).
// Requires the wasm package to be built:
//   wasm-pack build crates/pdfmuse-wasm --target nodejs --out-dir pkg
const fs = require("fs");
const path = require("path");

const wasm = require(path.resolve(__dirname, "../../crates/pdfmuse-wasm/pkg"));
const file = process.argv[2];
process.stdout.write(wasm.parse(new Uint8Array(fs.readFileSync(file))));
