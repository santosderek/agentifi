use eframe::egui::{self, Color32, Pos2, Stroke, Ui, Vec2};
pub fn agentifi_mark(ui: &mut Ui, size: f32) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(size * 1.15, size), egui::Sense::hover());
    let p = ui.painter();
    let x = rect.left();
    let y = rect.top();
    let blue = Color32::from_rgb(45, 145, 245);
    let light = Color32::from_rgb(92, 190, 255);
    let s = Stroke::new(size * 0.12, blue);
    p.line_segment(
        [
            Pos2::new(x + size * 0.12, y + size * 0.82),
            Pos2::new(x + size * 0.48, y + size * 0.08),
        ],
        s,
    );
    p.line_segment(
        [
            Pos2::new(x + size * 0.48, y + size * 0.08),
            Pos2::new(x + size * 0.9, y + size * 0.82),
        ],
        s,
    );
    p.line_segment(
        [
            Pos2::new(x + size * 0.30, y + size * 0.48),
            Pos2::new(x + size * 0.70, y + size * 0.48),
        ],
        Stroke::new(size * 0.1, light),
    );
    p.line_segment(
        [
            Pos2::new(x + size * 0.04, y + size * 0.84),
            Pos2::new(x + size * 0.28, y + size * 0.84),
        ],
        Stroke::new(size * 0.09, light),
    );
    response
}
