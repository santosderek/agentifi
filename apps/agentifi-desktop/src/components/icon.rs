use eframe::egui::{self, Color32, Pos2, Stroke, Ui, Vec2};
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum Icon {
    Overview,
    Explorer,
    Board,
    Refresh,
    Back,
    Folder,
    Search,
}
pub fn paint(ui: &mut Ui, icon: Icon, color: Color32, size: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let p = ui.painter();
    let c = rect.center();
    let s = size * 0.34;
    let stroke = Stroke::new((size / 10.0).max(1.2), color);
    match icon {
        Icon::Overview => {
            p.rect_stroke(
                egui::Rect::from_center_size(c, Vec2::splat(s * 1.5)),
                2.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            p.line_segment(
                [Pos2::new(c.x - s * 0.7, c.y), Pos2::new(c.x + s * 0.7, c.y)],
                stroke,
            );
        }
        Icon::Explorer => {
            for y in [-0.55, 0.0, 0.55] {
                p.line_segment(
                    [
                        Pos2::new(c.x - s, c.y + y * s),
                        Pos2::new(c.x + s, c.y + y * s),
                    ],
                    stroke,
                );
            }
        }
        Icon::Board => {
            for (x, y) in [(-0.55, -0.55), (0.55, -0.55), (-0.55, 0.55), (0.55, 0.55)] {
                p.rect_stroke(
                    egui::Rect::from_center_size(
                        Pos2::new(c.x + x * s, c.y + y * s),
                        Vec2::splat(s * 0.65),
                    ),
                    1.5,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }
        Icon::Refresh => {
            p.circle_stroke(c, s, stroke);
            p.line_segment(
                [
                    Pos2::new(c.x + s * 0.7, c.y - s * 0.7),
                    Pos2::new(c.x + s * 0.7, c.y - s * 0.05),
                ],
                stroke,
            );
        }
        Icon::Back => {
            p.line_segment([Pos2::new(c.x + s, c.y), Pos2::new(c.x - s, c.y)], stroke);
            p.line_segment(
                [
                    Pos2::new(c.x - s, c.y),
                    Pos2::new(c.x - 0.2 * s, c.y - 0.8 * s),
                ],
                stroke,
            );
            p.line_segment(
                [
                    Pos2::new(c.x - s, c.y),
                    Pos2::new(c.x - 0.2 * s, c.y + 0.8 * s),
                ],
                stroke,
            );
        }
        Icon::Folder => {
            p.rect_stroke(
                egui::Rect::from_center_size(c, Vec2::new(s * 2.0, s * 1.35)),
                2.0,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        Icon::Search => {
            p.circle_stroke(Pos2::new(c.x - s * 0.25, c.y - s * 0.2), s * 0.65, stroke);
            p.line_segment(
                [
                    Pos2::new(c.x + s * 0.25, c.y + s * 0.3),
                    Pos2::new(c.x + s, c.y + s * 1.0),
                ],
                stroke,
            );
        }
    }
    response
}
