// Print the raw IR JSON for a file via the Node native addon.
// Used by the cross-binding parity gate (tests/parity/check.py).
const fs = require("fs");
const path = require("path");

const native = require(path.resolve(__dirname, "../../bindings/node/native"));
const file = process.argv[2];
process.stdout.write(native.parse_buffer(fs.readFileSync(file)));
