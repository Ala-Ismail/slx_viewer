use eframe::egui::{
    Align2, Color32, Event, FontId, Key, Modifiers, MouseWheelUnit, Painter, PointerButton, Pos2,
    Rect, Sense, Stroke, StrokeKind, Ui, Vec2, vec2,
};
use slx_viewer::model::System;

use super::{scene::Scene, theme};

const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 8.0;

#[derive(Clone, Copy)]
pub enum ViewAction {
    Fit,
    Reset,
    Scale(f32),
}

pub struct View {
    zoom: f32,
    center: Pos2,
    pub selected: Option<usize>,
    pub action: Option<ViewAction>,
    pub reveal: Option<usize>,
    pan_primary: bool,
}

impl Default for View {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            center: Pos2::ZERO,
            selected: None,
            action: Some(ViewAction::Fit),
            reveal: None,
            pan_primary: false,
        }
    }
}

impl View {
    pub fn percent(&self) -> f32 {
        self.zoom * 100.0
    }

    pub fn screen(&self, world: Pos2, canvas: Rect) -> Pos2 {
        canvas.center() + (world - self.center) * self.zoom
    }

    fn world(&self, screen: Pos2, canvas: Rect) -> Pos2 {
        self.center + (screen - canvas.center()) / self.zoom
    }

    fn zoom_to(&mut self, zoom: f32, anchor: Pos2, canvas: Rect) {
        let fixed = self.world(anchor, canvas);
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        self.center = fixed - (anchor - canvas.center()) / self.zoom;
    }

    pub fn screen_rect(&self, bounds: Rect, canvas: Rect) -> Rect {
        Rect::from_two_pos(
            self.screen(bounds.min, canvas),
            self.screen(bounds.max, canvas),
        )
    }

    pub fn scale(&self) -> f32 {
        self.zoom
    }

    fn fit(&mut self, canvas: Rect, bounds: Rect) {
        if bounds.is_finite() && !bounds.is_negative() {
            self.center = bounds.center();
            let available = (canvas.size() - Vec2::splat(48.0)).max(Vec2::splat(1.0));
            self.zoom = (available.x / bounds.width().max(1.0))
                .min(available.y / bounds.height().max(1.0))
                .clamp(MIN_ZOOM, MAX_ZOOM);
        } else {
            self.center = Pos2::ZERO;
            self.zoom = 1.0;
        }
    }
}

pub fn show(ui: &mut Ui, system: &System, scene: &Scene, view: &mut View) -> Option<usize> {
    let reveal = view.reveal.take().and_then(|index| scene.blocks[index]);
    let frame = interact(
        ui,
        view,
        |painter| {
            let mut bounds = scene.bounds.unwrap_or(Rect::NOTHING);
            for (at, text) in &scene.labels {
                let label = painter.layout_no_wrap(
                    text.clone(),
                    FontId::proportional(12.0),
                    Color32::WHITE,
                );
                let top_left = *at - vec2(label.size().x / 2.0, 0.0);
                bounds = bounds.union(Rect::from_min_size(top_left, label.size()));
            }
            bounds
        },
        reveal,
        |point, _| {
            scene
                .blocks
                .iter()
                .rposition(|bounds| bounds.is_some_and(|bounds| bounds.contains(point)))
        },
    );
    paint(ui, &frame.painter, frame.rect, system, scene, view);
    frame.enter.then_some(view.selected).flatten()
}

pub(super) struct CanvasFrame {
    pub painter: Painter,
    pub rect: Rect,
    pub enter: bool,
}

pub(super) fn interact(
    ui: &mut Ui,
    view: &mut View,
    fit_bounds: impl FnOnce(&Painter) -> Rect,
    reveal: Option<Rect>,
    hit: impl Fn(Pos2, f32) -> Option<usize>,
) -> CanvasFrame {
    let previous_zoom = view.zoom;
    let previous_selection = view.selected;
    let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let canvas = response.rect;
    match view.action.take() {
        Some(ViewAction::Fit) => view.fit(canvas, fit_bounds(&painter)),
        Some(ViewAction::Reset) => view.zoom_to(1.0, canvas.center(), canvas),
        Some(ViewAction::Scale(factor)) => {
            view.zoom_to(view.zoom * factor, canvas.center(), canvas)
        }
        None => {}
    }
    if let Some(bounds) = reveal {
        response.request_focus();
        if !canvas.contains_rect(view.screen_rect(bounds, canvas)) {
            view.center = bounds.center();
        }
    }
    if response.hovered()
        && let Some(pointer) = response.hover_pos()
    {
        ui.input(|input| {
            for event in &input.events {
                if let Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } = event
                {
                    let scale = match unit {
                        MouseWheelUnit::Point => 1.0,
                        MouseWheelUnit::Line => 40.0,
                        MouseWheelUnit::Page => canvas.height().max(1.0),
                    };
                    if modifiers.ctrl {
                        let factor = (delta.y * scale * 0.004).clamp(-4.0, 4.0).exp();
                        view.zoom_to(view.zoom * factor, pointer, canvas);
                    } else {
                        let scroll = if modifiers.shift {
                            vec2(if delta.x == 0.0 { delta.y } else { delta.x }, 0.0)
                        } else {
                            *delta
                        };
                        view.center -= scroll * (scale / view.zoom);
                    }
                }
            }
        });
    }
    if response.drag_started_by(PointerButton::Primary) {
        view.pan_primary = ui
            .input(|input| input.pointer.press_origin())
            .is_some_and(|start| hit(view.world(start, canvas), view.zoom).is_none());
    }
    if response.dragged_by(PointerButton::Middle)
        || (view.pan_primary && response.dragged_by(PointerButton::Primary))
    {
        view.center -= response.drag_delta() / view.zoom;
    }
    if response.drag_stopped_by(PointerButton::Primary) {
        view.pan_primary = false;
    }
    if response.clicked_by(PointerButton::Primary) {
        response.request_focus();
        view.selected = response
            .interact_pointer_pos()
            .and_then(|point| hit(view.world(point, canvas), view.zoom));
    }
    let enter = response.double_clicked_by(PointerButton::Primary)
        || (response.has_focus()
            && ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter)));
    if previous_zoom != view.zoom || previous_selection != view.selected {
        ui.ctx().request_repaint();
    }
    CanvasFrame {
        painter,
        rect: canvas,
        enter,
    }
}

fn paint(ui: &Ui, painter: &Painter, canvas: Rect, system: &System, scene: &Scene, view: &View) {
    let visuals = ui.visuals();
    let text = visuals.strong_text_color();
    painter.rect_filled(canvas, 0, visuals.extreme_bg_color);
    for path in &scene.wires {
        for pair in path.windows(2) {
            let start = view.screen(pair[0], canvas);
            let end = view.screen(pair[1], canvas);
            if canvas.intersects(Rect::from_two_pos(start, end)) {
                painter.line_segment([start, end], Stroke::new(1.25, theme::WIRE));
            }
        }
    }
    for (index, bounds) in scene.blocks.iter().enumerate() {
        let Some(bounds) = bounds.map(|bounds| view.screen_rect(bounds, canvas)) else {
            continue;
        };
        if !canvas.intersects(bounds) {
            continue;
        }
        let selected = view.selected == Some(index);
        let stroke = if selected {
            Stroke::new(2.0, visuals.selection.stroke.color)
        } else {
            Stroke::new(1.0, theme::OUTLINE)
        };
        painter.rect_filled(
            bounds,
            2,
            if selected {
                visuals.selection.bg_fill
            } else {
                theme::OBJECT
            },
        );
        painter.rect_stroke(bounds, 2, stroke, StrokeKind::Inside);
        if bounds.height() >= 13.0 && bounds.width() >= 12.0 {
            let block = &system.blocks[index];
            let symbol = if block.chart.is_some() {
                "Stateflow"
            } else {
                match block.block_type.as_str() {
                    "Sum" | "Product" if block.properties.contains_key("Inputs") => "",
                    "Inport" => "In",
                    "Outport" => "Out",
                    "Constant" => block.properties.get("Value").map_or("1", String::as_str),
                    "Gain" => "K",
                    "Sum" => "Σ",
                    "SubSystem" => "Subsystem",
                    "Integrator" => "1/s",
                    _ => &block.block_type,
                }
            };
            painter.with_clip_rect(canvas.intersect(bounds)).text(
                bounds.center(),
                Align2::CENTER_CENTER,
                symbol,
                FontId::proportional((view.zoom * 11.0).clamp(8.0, 20.0)),
                text,
            );
        }
    }
    if view.zoom >= 0.35 {
        for (at, operator) in &scene.operators {
            let at = view.screen(*at, canvas);
            if canvas.contains(at) {
                painter.text(
                    at,
                    Align2::CENTER_CENTER,
                    operator,
                    FontId::proportional((view.zoom * 11.0).clamp(8.0, 20.0)),
                    text,
                );
            }
        }

        for &port in &scene.ports {
            let port = view.screen(port, canvas);
            if canvas.contains(port) {
                painter.circle_filled(port, 2.0, text);
            }
        }
        let font = FontId::proportional((12.0 * view.zoom).clamp(7.0, 34.0));
        for (at, label) in &scene.labels {
            let at = view.screen(*at, canvas);
            if canvas.expand(100.0).contains(at) {
                painter.text(at, Align2::CENTER_TOP, label, font.clone(), text);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::pos2;

    #[test]
    fn zoom_keeps_pointer_anchor_at_both_limits() {
        let canvas = Rect::from_min_size(pos2(200.0, 50.0), vec2(800.0, 600.0));
        let anchor = pos2(321.0, 456.0);
        let mut view = View::default();
        let world = view.world(anchor, canvas);
        for zoom in [2.0, 0.001, 100.0, 1.0] {
            view.zoom_to(zoom, anchor, canvas);
            assert!(view.screen(world, canvas).distance(anchor) < 1.0);
            assert_eq!(view.zoom, zoom.clamp(MIN_ZOOM, MAX_ZOOM));
        }
    }
}
