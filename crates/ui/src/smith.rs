use egui::{Pos2, Sense, Shape, Stroke, StrokeKind, Ui, Vec2};
use egui_bench::prelude::*;
use testbench_core::antenna_vna::{C64, Point};
use testbench_core::smith::{GRID_R, GRID_X, reactance_arc, resistance_circle, to_screen};

pub fn show(ui: &mut Ui, points: &[Point], marker: Option<usize>, size: f32) {
    let side = size.min(ui.available_width());
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 1.0, WELL);
    p.rect_stroke(rect, 1.0, Stroke::new(1.0, ETCH), StrokeKind::Inside);
    let c = rect.center();
    let radius = (side * 0.5 - 8.0) as f64;
    let centre = (c.x as f64, c.y as f64);
    let at = |g: C64| {
        let (x, y) = to_screen(g, centre, radius);
        Pos2::new(x as f32, y as f32)
    };
    let grid = Stroke::new(1.0, ETCH);
    p.circle_stroke(c, radius as f32, Stroke::new(1.0, LEGEND.gamma_multiply(0.6)));
    p.line_segment([at(C64::new(-1.0, 0.0)), at(C64::new(1.0, 0.0))], grid);
    for r in GRID_R {
        let (centre_g, rad) = resistance_circle(r);
        let mid = at(centre_g);
        p.circle_stroke(mid, (rad * radius) as f32, grid);
    }
    for x in GRID_X {
        for sign in [1.0, -1.0] {
            let pts: Vec<Pos2> = reactance_arc(sign * x, 60).into_iter().map(at).collect();
            p.add(Shape::line(pts, grid));
        }
    }
    let trace: Vec<Pos2> = points.iter().map(|pt| at(pt.s11)).collect();
    if trace.len() > 1 {
        p.add(Shape::line(trace, Stroke::new(1.5, TRACE)));
    }
    if let Some(pt) = marker.and_then(|i| points.get(i)) {
        p.circle_filled(at(pt.s11), 4.0, READOUT);
    }
}
