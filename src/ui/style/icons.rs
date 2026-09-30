//! Small vector icons painted with egui shapes. Each takes the icon's square rect.

use eframe::egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};
use std::f32::consts::{FRAC_PI_2, PI};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Play,
    Pause,
    Prev,
    Next,
    Shuffle,
    Volume,
    Mute,
    AirPlay,
    Computer,
    Search,
    Home,
    Heart,
    Grid,
    Refresh,
    Note,
    Sun,
    Moon,
    Check,
    LogOut,
    Queue,
    Close,
    /// Small up/down chevrons: "opens a menu".
    Chevrons,
}

fn p(r: Rect, x: f32, y: f32) -> Pos2 {
    pos2(r.left() + x * r.width(), r.top() + y * r.height())
}

fn arc(center: Pos2, radius: f32, from: f32, to: f32) -> Vec<Pos2> {
    let n = 16;
    (0..=n).map(|i| from + (to - from) * i as f32 / n as f32).map(|a| center + vec2(a.cos(), a.sin()) * radius).collect()
}

/// Crescent outline: a disc of radius `big` minus a disc offset towards the upper right.
/// Returns the outer arc followed by the inner (bite) arc, as one closed outline.
fn crescent(center: Pos2, big: f32) -> Vec<Pos2> {
    let small = big * 0.82;
    let theta = -PI / 4.0; // direction of the bite
    let d = big * 0.62; // distance between the two centres
    let dir = vec2(theta.cos(), theta.sin());
    let bite = center + dir * d;
    // Intersection points, measured from each centre.
    let x = (d * d + big * big - small * small) / (2.0 * d);
    let alpha = (x / big).clamp(-1.0, 1.0).acos();
    let beta = ((x - d) / small).clamp(-1.0, 1.0).acos();
    let mut pts = arc(center, big, theta + alpha, theta + 2.0 * PI - alpha);
    pts.extend(arc(bite, small, theta - beta, theta + beta).into_iter().skip(1));
    pts.pop();
    pts
}

pub fn paint(painter: &Painter, r: Rect, icon: Icon, color: Color32) {
    let w = r.width();
    let stroke = Stroke::new((w * 0.09).max(1.4), color);
    let line = |a: Pos2, b: Pos2| painter.line_segment([a, b], stroke);
    let poly = |pts: Vec<Pos2>| painter.add(Shape::convex_polygon(pts, color, Stroke::NONE));
    match icon {
        Icon::Play => {
            poly(vec![p(r, 0.26, 0.14), p(r, 0.86, 0.5), p(r, 0.26, 0.86)]);
        }
        Icon::Pause => {
            painter.rect_filled(Rect::from_min_max(p(r, 0.22, 0.16), p(r, 0.42, 0.84)), 1.5, color);
            painter.rect_filled(Rect::from_min_max(p(r, 0.58, 0.16), p(r, 0.78, 0.84)), 1.5, color);
        }
        Icon::Prev | Icon::Next => {
            let flip = |x: f32| if icon == Icon::Next { 1.0 - x } else { x };
            painter.rect_filled(Rect::from_two_pos(p(r, flip(0.18), 0.2), p(r, flip(0.28), 0.8)), 1.0, color);
            poly(if icon == Icon::Next {
                vec![p(r, flip(0.86), 0.2), p(r, flip(0.3), 0.5), p(r, flip(0.86), 0.8)]
            } else {
                vec![p(r, 0.3, 0.5), p(r, 0.86, 0.2), p(r, 0.86, 0.8)]
            });
        }
        Icon::Shuffle => {
            line(p(r, 0.12, 0.3), p(r, 0.35, 0.3));
            line(p(r, 0.35, 0.3), p(r, 0.62, 0.7));
            line(p(r, 0.62, 0.7), p(r, 0.86, 0.7));
            line(p(r, 0.12, 0.7), p(r, 0.35, 0.7));
            line(p(r, 0.35, 0.7), p(r, 0.62, 0.3));
            line(p(r, 0.62, 0.3), p(r, 0.86, 0.3));
            for y in [0.3, 0.7] {
                line(p(r, 0.76, y - 0.1), p(r, 0.88, y));
                line(p(r, 0.76, y + 0.1), p(r, 0.88, y));
            }
        }
        Icon::Volume | Icon::Mute => {
            poly(vec![p(r, 0.12, 0.38), p(r, 0.28, 0.38), p(r, 0.5, 0.18), p(r, 0.5, 0.82), p(r, 0.28, 0.62), p(r, 0.12, 0.62)]);
            if icon == Icon::Mute {
                line(p(r, 0.62, 0.36), p(r, 0.88, 0.64));
                line(p(r, 0.88, 0.36), p(r, 0.62, 0.64));
            } else {
                let c = p(r, 0.5, 0.5);
                painter.add(Shape::line(arc(c, w * 0.18, -0.9, 0.9), stroke));
                painter.add(Shape::line(arc(c, w * 0.34, -0.9, 0.9), stroke));
            }
        }
        Icon::AirPlay => {
            let s = Stroke::new(stroke.width, color);
            painter.add(Shape::line(vec![p(r, 0.3, 0.68), p(r, 0.12, 0.68), p(r, 0.12, 0.16), p(r, 0.88, 0.16), p(r, 0.88, 0.68), p(r, 0.7, 0.68)], s));
            poly(vec![p(r, 0.5, 0.56), p(r, 0.76, 0.88), p(r, 0.24, 0.88)]);
        }
        Icon::Computer => {
            painter.rect_stroke(Rect::from_min_max(p(r, 0.18, 0.2), p(r, 0.82, 0.66)), 2.0, stroke, eframe::egui::StrokeKind::Middle);
            line(p(r, 0.06, 0.8), p(r, 0.94, 0.8));
        }
        Icon::Search => {
            painter.circle_stroke(p(r, 0.43, 0.43), w * 0.26, stroke);
            line(p(r, 0.62, 0.62), p(r, 0.86, 0.86));
        }
        Icon::Home => {
            let s = Stroke::new(stroke.width, color);
            painter.add(Shape::closed_line(
                vec![p(r, 0.5, 0.12), p(r, 0.88, 0.44), p(r, 0.78, 0.44), p(r, 0.78, 0.86), p(r, 0.22, 0.86), p(r, 0.22, 0.44), p(r, 0.12, 0.44)],
                s,
            ));
        }
        Icon::Heart => {
            let rr = w * 0.2;
            painter.circle_filled(p(r, 0.33, 0.38), rr, color);
            painter.circle_filled(p(r, 0.67, 0.38), rr, color);
            poly(vec![p(r, 0.15, 0.46), p(r, 0.85, 0.46), p(r, 0.5, 0.86)]);
        }
        Icon::Grid => {
            for (x, y) in [(0.14, 0.14), (0.54, 0.14), (0.14, 0.54), (0.54, 0.54)] {
                painter.rect_filled(Rect::from_min_size(p(r, x, y), vec2(w * 0.32, w * 0.32)), 2.0, color);
            }
        }
        Icon::Refresh => {
            let c = p(r, 0.5, 0.5);
            painter.add(Shape::line(arc(c, w * 0.32, -FRAC_PI_2 + 0.5, PI * 1.5 - 0.2), stroke));
            let tip = c + vec2((-FRAC_PI_2 + 0.5_f32).cos(), (-FRAC_PI_2 + 0.5_f32).sin()) * w * 0.32;
            poly(vec![tip + vec2(-w * 0.16, -w * 0.08), tip + vec2(w * 0.1, -w * 0.1), tip + vec2(-w * 0.02, w * 0.14)]);
        }
        Icon::Sun => {
            let c = p(r, 0.5, 0.5);
            painter.circle_filled(c, w * 0.17, color);
            for k in 0..8 {
                let a = k as f32 * PI / 4.0;
                let d = vec2(a.cos(), a.sin());
                line(c + d * w * 0.3, c + d * w * 0.42);
            }
        }
        Icon::Moon => {
            // Concave, so outlined rather than filled (egui fills convex shapes only).
            painter.add(Shape::closed_line(crescent(p(r, 0.5, 0.5), w * 0.36), stroke));
        }
        Icon::Check => {
            painter.add(Shape::line(vec![p(r, 0.18, 0.52), p(r, 0.4, 0.74), p(r, 0.84, 0.28)], stroke));
        }
        Icon::LogOut => {
            let s = Stroke::new(stroke.width, color);
            painter.add(Shape::line(vec![p(r, 0.5, 0.14), p(r, 0.16, 0.14), p(r, 0.16, 0.86), p(r, 0.5, 0.86)], s));
            line(p(r, 0.4, 0.5), p(r, 0.88, 0.5));
            line(p(r, 0.72, 0.34), p(r, 0.88, 0.5));
            line(p(r, 0.72, 0.66), p(r, 0.88, 0.5));
        }
        Icon::Queue => {
            for y in [0.24, 0.44, 0.64] {
                line(p(r, 0.12, y), p(r, 0.62, y));
            }
            line(p(r, 0.12, 0.84), p(r, 0.46, 0.84));
            poly(vec![p(r, 0.66, 0.6), p(r, 0.92, 0.76), p(r, 0.66, 0.92)]);
        }
        Icon::Close => {
            line(p(r, 0.22, 0.22), p(r, 0.78, 0.78));
            line(p(r, 0.78, 0.22), p(r, 0.22, 0.78));
        }
        Icon::Chevrons => {
            let s = Stroke::new(stroke.width, color);
            painter.add(Shape::line(vec![p(r, 0.28, 0.4), p(r, 0.5, 0.2), p(r, 0.72, 0.4)], s));
            painter.add(Shape::line(vec![p(r, 0.28, 0.6), p(r, 0.5, 0.8), p(r, 0.72, 0.6)], s));
        }
        Icon::Note => {
            painter.circle_filled(p(r, 0.36, 0.72), w * 0.13, color);
            line(p(r, 0.47, 0.72), p(r, 0.47, 0.2));
            line(p(r, 0.47, 0.2), p(r, 0.74, 0.28));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crescent_outline_closes() {
        let c = pos2(0.0, 0.0);
        let pts = crescent(c, 10.0);
        let outer_end = pts[16];
        let inner_start = pts[17];
        // Consecutive points never jump more than a small step: the arcs meet.
        for w in pts.windows(2) {
            assert!((w[1] - w[0]).length() < 4.5, "gap between {:?} and {:?}", w[0], w[1]);
        }
        assert!((outer_end - inner_start).length() < 4.5);
        assert!((pts[0] - *pts.last().unwrap()).length() < 4.5);
    }
}
