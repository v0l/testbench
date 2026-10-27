use egui::{Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use egui_bench::prelude::{ETCH, LEGEND, WELL, mono};
use testbench_core::axis::nice_ticks;

pub fn map(v: f64, lo: f64, hi: f64, a: f32, b: f32) -> f32 {
    if hi <= lo {
        return a;
    }
    a + ((v - lo) / (hi - lo)) as f32 * (b - a)
}

pub fn unmap(px: f32, a: f32, b: f32, lo: f64, hi: f64) -> f64 {
    if (b - a).abs() < f32::EPSILON {
        return lo;
    }
    lo + ((px - a) / (b - a)) as f64 * (hi - lo)
}

pub struct Series<'a> {
    pub points: &'a [(f64, f64)],
    pub tint: Color32,
}

pub struct Plot<'a> {
    pub size: Vec2,
    pub x: (f64, f64),
    pub y: (f64, f64),
    pub series: Vec<Series<'a>>,
    pub marker: Option<f64>,
    pub marker_tint: Color32,
    pub x_label: fn(f64) -> String,
    pub y_label: fn(f64) -> String,
}

pub struct PlotResult {
    pub clicked_x: Option<f64>,
}

const LEFT: f32 = 44.0;
const BOTTOM: f32 = 16.0;

impl Plot<'_> {
    pub fn show(self, ui: &mut Ui) -> PlotResult {
        let size = Vec2::new(self.size.x.min(ui.available_width()), self.size.y);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 1.0, WELL);
        p.rect_stroke(rect, 1.0, Stroke::new(1.0, ETCH), StrokeKind::Inside);
        let area = Rect::from_min_max(
            Pos2::new(rect.left() + LEFT, rect.top() + 6.0),
            Pos2::new(rect.right() - 8.0, rect.bottom() - BOTTOM),
        );
        let font = mono(10.0);
        for t in nice_ticks(self.y.0, self.y.1, 4) {
            let y = map(t, self.y.0, self.y.1, area.bottom(), area.top());
            p.line_segment([Pos2::new(area.left(), y), Pos2::new(area.right(), y)], Stroke::new(1.0, ETCH));
            p.text(
                Pos2::new(area.left() - 4.0, y),
                egui::Align2::RIGHT_CENTER,
                (self.y_label)(t),
                font.clone(),
                LEGEND,
            );
        }
        for t in nice_ticks(self.x.0, self.x.1, 6) {
            let x = map(t, self.x.0, self.x.1, area.left(), area.right());
            p.line_segment([Pos2::new(x, area.top()), Pos2::new(x, area.bottom())], Stroke::new(1.0, ETCH));
            p.text(
                Pos2::new(x, area.bottom() + 3.0),
                egui::Align2::CENTER_TOP,
                (self.x_label)(t),
                font.clone(),
                LEGEND,
            );
        }
        let clipped = ui.painter_at(area);
        for s in &self.series {
            let pts: Vec<Pos2> = s
                .points
                .iter()
                .map(|(x, y)| {
                    Pos2::new(
                        map(*x, self.x.0, self.x.1, area.left(), area.right()),
                        map(*y, self.y.0, self.y.1, area.bottom(), area.top()),
                    )
                })
                .collect();
            if pts.len() > 1 {
                clipped.add(egui::Shape::line(pts, Stroke::new(1.5, s.tint)));
            }
        }
        if let Some(m) = self.marker {
            let x = map(m, self.x.0, self.x.1, area.left(), area.right());
            if x >= area.left() && x <= area.right() {
                clipped.line_segment(
                    [Pos2::new(x, area.top()), Pos2::new(x, area.bottom())],
                    Stroke::new(1.0, self.marker_tint),
                );
            }
        }
        let at = |pos: Pos2| {
            area.contains(pos).then(|| unmap(pos.x, area.left(), area.right(), self.x.0, self.x.1))
        };
        PlotResult {
            clicked_x: if resp.clicked() { resp.interact_pointer_pos().and_then(at) } else { None },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_and_unmap_are_inverse() {
        let px = map(7.5, 5.0, 10.0, 100.0, 200.0);
        assert_eq!(px, 150.0);
        assert!((unmap(px, 100.0, 200.0, 5.0, 10.0) - 7.5).abs() < 1e-9);
    }

    #[test]
    fn degenerate_ranges_do_not_divide_by_zero() {
        assert_eq!(map(3.0, 1.0, 1.0, 10.0, 20.0), 10.0);
        assert_eq!(unmap(5.0, 4.0, 4.0, 1.0, 2.0), 1.0);
    }

    #[test]
    fn y_axis_runs_bottom_to_top() {
        assert_eq!(map(0.0, 0.0, 10.0, 200.0, 100.0), 200.0);
        assert_eq!(map(10.0, 0.0, 10.0, 200.0, 100.0), 100.0);
    }
}
