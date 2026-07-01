//! Ruled-table reconstruction.
//!
//! Builds a table grid from the vector rules/rects collected by the interpreter:
//! horizontal segments give row lines, vertical segments give column lines, their
//! crossings define cells, and chars are dropped into the cell that contains
//! them. Merged cells are inferred from **missing** interior borders (a missing
//! vertical border ⇒ a horizontal span; a missing horizontal border ⇒ a vertical
//! span). Highest-precision table path; whitespace-aligned tables are PER-55.

use crate::ir::{BBox, Cell, Char, Rect, Rule, Table, TableSource};

/// Clustering / coverage tolerance in points.
const EPS: f32 = 2.0;
/// Space is inserted between chars in a cell when the gap exceeds this × size.
const SPACE_GAP: f32 = 0.25;

/// An axis-aligned line segment: `pos` is the constant coordinate, `lo..hi` the
/// span along the other axis.
struct Seg {
    pos: f32,
    lo: f32,
    hi: f32,
}

/// Detect ruled tables on a page. Returns at most one table (the bounding grid);
/// multiple disjoint tables per page is future work.
pub(super) fn detect_ruled(chars: &[Char], rects: &[Rect], rules: &[Rule]) -> Vec<Table> {
    let (hsegs, vsegs) = collect(rects, rules);
    let ys = cluster_positions(&hsegs);
    let xs = cluster_positions(&vsegs);
    // Need a real grid: ≥2 lines each way and at least one interior divider.
    if xs.len() < 2 || ys.len() < 2 || (xs.len() < 3 && ys.len() < 3) {
        return Vec::new();
    }
    vec![build_table(chars, &xs, &ys, &hsegs, &vsegs)]
}

fn collect(rects: &[Rect], rules: &[Rule]) -> (Vec<Seg>, Vec<Seg>) {
    let mut h = Vec::new();
    let mut v = Vec::new();
    let mut push = |x0: f32, y0: f32, x1: f32, y1: f32| {
        if (y0 - y1).abs() <= EPS {
            h.push(Seg { pos: (y0 + y1) / 2.0, lo: x0.min(x1), hi: x0.max(x1) });
        } else if (x0 - x1).abs() <= EPS {
            v.push(Seg { pos: (x0 + x1) / 2.0, lo: y0.min(y1), hi: y0.max(y1) });
        }
    };
    for r in rules {
        push(r.x0, r.y0, r.x1, r.y1);
    }
    for rc in rects {
        let b = rc.bbox;
        push(b.x0, b.y0, b.x1, b.y0); // top
        push(b.x0, b.y1, b.x1, b.y1); // bottom
        push(b.x0, b.y0, b.x0, b.y1); // left
        push(b.x1, b.y0, b.x1, b.y1); // right
    }
    (h, v)
}

/// Distinct line positions, clustering values within `EPS`.
fn cluster_positions(segs: &[Seg]) -> Vec<f32> {
    let mut ps: Vec<f32> = segs.iter().map(|s| s.pos).collect();
    ps.sort_by(|a, b| a.total_cmp(b));
    let mut out: Vec<f32> = Vec::new();
    for p in ps {
        match out.last() {
            Some(&last) if (p - last).abs() <= EPS => {}
            _ => out.push(p),
        }
    }
    out
}

fn build_table(chars: &[Char], xs: &[f32], ys: &[f32], hsegs: &[Seg], vsegs: &[Seg]) -> Table {
    let ncol = xs.len() - 1;
    let nrow = ys.len() - 1;
    let mut covered = vec![vec![false; ncol]; nrow];
    let mut rows: Vec<Vec<Cell>> = Vec::with_capacity(nrow);

    for r in 0..nrow {
        let mut row_cells = Vec::new();
        for c in 0..ncol {
            if covered[r][c] {
                continue;
            }
            // Horizontal span: extend right while the interior vertical border is absent.
            let mut cspan = 1;
            while c + cspan < ncol && !has_seg(vsegs, xs[c + cspan], ys[r], ys[r + 1]) {
                cspan += 1;
            }
            // Vertical span: extend down while the border below is absent across the span.
            let mut rspan = 1;
            'grow: while r + rspan < nrow {
                for cc in c..c + cspan {
                    if has_seg(hsegs, ys[r + rspan], xs[cc], xs[cc + 1]) {
                        break 'grow;
                    }
                }
                rspan += 1;
            }
            for row in covered.iter_mut().take(r + rspan).skip(r) {
                for cell in row.iter_mut().take(c + cspan).skip(c) {
                    *cell = true;
                }
            }
            let bbox = BBox { x0: xs[c], y0: ys[r], x1: xs[c + cspan], y1: ys[r + rspan] };
            row_cells.push(Cell {
                text: cell_text(chars, bbox),
                bbox,
                row_span: rspan as u16,
                col_span: cspan as u16,
            });
        }
        rows.push(row_cells);
    }

    Table {
        bbox: BBox { x0: xs[0], y0: ys[0], x1: xs[xs.len() - 1], y1: ys[ys.len() - 1] },
        rows,
        source: TableSource::Ruled,
    }
}

/// Is there a segment at `pos` covering the whole `lo..hi` range?
fn has_seg(segs: &[Seg], pos: f32, lo: f32, hi: f32) -> bool {
    segs.iter().any(|s| (s.pos - pos).abs() <= EPS && s.lo <= lo + EPS && s.hi >= hi - EPS)
}

/// Concatenate the chars whose center falls in `cell`, left-to-right, inserting
/// spaces on wide gaps (same rule as line building).
fn cell_text(chars: &[Char], cell: BBox) -> String {
    let mut inside: Vec<&Char> = chars
        .iter()
        .filter(|c| {
            let cx = (c.bbox.x0 + c.bbox.x1) / 2.0;
            let cy = (c.bbox.y0 + c.bbox.y1) / 2.0;
            cx >= cell.x0 && cx <= cell.x1 && cy >= cell.y0 && cy <= cell.y1
        })
        .collect();
    inside.sort_by(|a, b| a.bbox.x0.total_cmp(&b.bbox.x0));

    let mut text = String::new();
    let mut prev_x1: Option<f32> = None;
    for c in inside {
        if let Some(px1) = prev_x1 {
            if c.bbox.x0 - px1 > SPACE_GAP * c.size.max(1.0) {
                text.push(' ');
            }
        }
        text.push_str(&c.text);
        prev_x1 = Some(c.bbox.x1);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::FontRef;

    fn ch(text: &str, cx: f32, cy: f32) -> Char {
        Char {
            text: text.into(),
            bbox: BBox { x0: cx - 3.0, y0: cy - 5.0, x1: cx + 3.0, y1: cy + 5.0 },
            font: FontRef { name: "F".into() },
            size: 10.0,
            color: None,
        }
    }
    fn vrule(x: f32, y0: f32, y1: f32) -> Rule {
        Rule { x0: x, y0, x1: x, y1, width: 1.0 }
    }
    fn hrule(y: f32, x0: f32, x1: f32) -> Rule {
        Rule { x0, y0: y, x1, y1: y, width: 1.0 }
    }

    #[test]
    fn reconstructs_2x2_grid() {
        // Grid lines at x∈{0,50,100}, y∈{0,20,40}; one char centered in each cell.
        let rules = vec![
            vrule(0.0, 0.0, 40.0), vrule(50.0, 0.0, 40.0), vrule(100.0, 0.0, 40.0),
            hrule(0.0, 0.0, 100.0), hrule(20.0, 0.0, 100.0), hrule(40.0, 0.0, 100.0),
        ];
        let chars = vec![ch("A", 25.0, 10.0), ch("B", 75.0, 10.0), ch("C", 25.0, 30.0), ch("D", 75.0, 30.0)];
        let tables = detect_ruled(&chars, &[], &rules);
        assert_eq!(tables.len(), 1);
        let t = &tables[0];
        assert_eq!(t.source, TableSource::Ruled);
        assert_eq!(t.rows.len(), 2);
        let texts: Vec<Vec<&str>> = t.rows.iter().map(|r| r.iter().map(|c| c.text.as_str()).collect()).collect();
        assert_eq!(texts, vec![vec!["A", "B"], vec!["C", "D"]]);
        assert!(t.rows.iter().flatten().all(|c| c.row_span == 1 && c.col_span == 1));
    }

    #[test]
    fn merges_cells_on_missing_interior_border() {
        // Top row has no divider at x=50 (vertical there only spans the bottom row),
        // so the top row is one merged cell spanning both columns.
        let rules = vec![
            vrule(0.0, 0.0, 40.0), vrule(50.0, 20.0, 40.0), vrule(100.0, 0.0, 40.0),
            hrule(0.0, 0.0, 100.0), hrule(20.0, 0.0, 100.0), hrule(40.0, 0.0, 100.0),
        ];
        let chars = vec![ch("A", 25.0, 10.0), ch("B", 75.0, 10.0), ch("C", 25.0, 30.0), ch("D", 75.0, 30.0)];
        let t = &detect_ruled(&chars, &[], &rules)[0];
        assert_eq!(t.rows[0].len(), 1);
        assert_eq!(t.rows[0][0].col_span, 2);
        assert_eq!(t.rows[0][0].text, "A B");
        assert_eq!(t.rows[1].len(), 2);
    }

    #[test]
    fn no_grid_without_interior_lines() {
        // A lone rectangle (just an outer box) is not a table.
        let rects = vec![Rect { bbox: BBox { x0: 0.0, y0: 0.0, x1: 100.0, y1: 40.0 } }];
        assert!(detect_ruled(&[], &rects, &[]).is_empty());
    }
}
