//! JSON rendering of the full IR.

use crate::ir::Document;

/// Serialize the entire [`Document`] to pretty-printed JSON.
///
/// The IR derives `Serialize` and contains only serializable types, so the
/// pretty path cannot realistically fail; on the impossible error we fall back
/// to compact `to_string` rather than panic.
pub fn to_json(doc: &Document) -> String {
    serde_json::to_string_pretty(doc)
        .or_else(|_| serde_json::to_string(doc))
        .unwrap_or_default()
}
