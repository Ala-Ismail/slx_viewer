use eframe::egui::{Align2, FontId, Pos2, Rect, Shape, Stroke, StrokeKind, Ui, vec2};
use slx_viewer::model::stateflow::{
    ChartEndpoint, ChartEndpointTarget, ChartNodeGeometry, ChartNodeKind, ChartTransition,
    StateflowChart,
};

use super::{
    canvas::{self, View},
    scene::{point, rect},
    theme,
};

pub struct ChartScene {
    nodes: Vec<Option<Rect>>,
    edges: Vec<Option<Edge>>,
    bounds: Rect,
    pub hidden_subviews: bool,
    pub missing_geometry: bool,
}

struct Edge {
    points: Vec<Pos2>,
    bounds: Rect,
    label: Option<Rect>,
    default: bool,
}

impl ChartScene {
    pub fn new(chart: &StateflowChart) -> Self {
        let mut scene = Self {
            nodes: Vec::with_capacity(chart.nodes.len()),
            edges: Vec::with_capacity(chart.transitions.len()),
            bounds: Rect::NOTHING,
            hidden_subviews: false,
            missing_geometry: false,
        };
        for node in &chart.nodes {
            if !scene.include_subviewer(node.properties.get("subviewer"), chart) {
                scene.nodes.push(None);
                continue;
            }
            let bounds = node.geometry.and_then(|geometry| match geometry {
                ChartNodeGeometry::Bounds(bounds) => rect(bounds),
                ChartNodeGeometry::Junction {
                    center: [x, y],
                    radius,
                } => Some(Rect::from_min_max(
                    point([x - radius, y - radius])?,
                    point([x + radius, y + radius])?,
                )),
            });
            if let Some(bounds) = bounds {
                scene.bounds = scene.bounds.union(bounds);
            } else {
                scene.missing_geometry = true;
            }
            scene.nodes.push(bounds);
        }
        for transition in &chart.transitions {
            if !scene.include_subviewer(transition.properties.get("subviewer"), chart) {
                scene.edges.push(None);
                continue;
            }
            let edge = Edge::new(transition);
            if let Some(edge) = &edge {
                scene.bounds = scene.bounds.union(edge.bounds);
                if let Some(label) = edge.label {
                    scene.bounds = scene.bounds.union(label);
                }
            } else {
                scene.missing_geometry = true;
            }
            scene.edges.push(edge);
        }
        scene
    }

    fn include_subviewer(&mut self, saved: Option<&String>, chart: &StateflowChart) -> bool {
        let separate =
            saved.is_some_and(|id| !id.trim().is_empty() && id.trim() != chart.source_id);
        self.hidden_subviews |= separate;
        !separate
    }

    fn hit(&self, point: Pos2, scale: f32) -> Option<usize> {
        let tolerance = 5.0 / scale;
        let mut nearest = tolerance * tolerance;
        let mut hit = None;
        for (index, edge) in self.edges.iter().enumerate() {
            let Some(edge) = edge else { continue };
            if edge.label.is_some_and(|label| label.contains(point)) {
                return Some(self.nodes.len() + index);
            }
            if edge.bounds.expand(tolerance).contains(point) {
                for pair in edge.points.windows(2) {
                    let distance = distance_sq(point, pair[0], pair[1]);
                    if distance <= nearest {
                        nearest = distance;
                        hit = Some(self.nodes.len() + index);
                    }
                }
            }
        }
        hit.or_else(|| {
            self.nodes
                .iter()
                .rposition(|bounds| bounds.is_some_and(|bounds| bounds.contains(point)))
        })
    }
}

impl Edge {
    fn new(transition: &ChartTransition) -> Option<Self> {
        let start = endpoint_position(&transition.source)?;
        let end = endpoint_position(&transition.destination)?;
        let points = if let Some(midpoint) = transition.midpoint.and_then(point) {
            // A quadratic passing through the saved midpoint at t = 1/2.
            let control = midpoint + (midpoint - start.lerp(end, 0.5));
            (0..=16)
                .map(|step| {
                    let t = step as f32 / 16.0;
                    start.lerp(control, t).lerp(control.lerp(end, t), t)
                })
                .collect()
        } else {
            vec![start, end]
        };
        let bounds = Rect::from_points(&points);
        let label = transition
            .label
            .as_ref()
            .filter(|label| !label.is_empty())
            .and_then(|_| transition.label_bounds.and_then(rect));
        Some(Self {
            points,
            bounds,
            label,
            default: transition.source.target == ChartEndpointTarget::Default,
        })
    }
}

fn endpoint_position(endpoint: &ChartEndpoint) -> Option<Pos2> {
    match endpoint.target {
        ChartEndpointTarget::Default | ChartEndpointTarget::Node { index: Some(_), .. } => {
            let saved = endpoint.intersection?;
            point([saved[4], saved[5]])
        }
        _ => None,
    }
}

fn distance_sq(point: Pos2, a: Pos2, b: Pos2) -> f32 {
    let delta = b - a;
    let length = delta.length_sq();
    if length == 0.0 {
        return point.distance_sq(a);
    }
    let fraction = ((point - a).dot(delta) / length).clamp(0.0, 1.0);
    point.distance_sq(a + delta * fraction)
}

pub fn show(ui: &mut Ui, chart: &StateflowChart, scene: &ChartScene, view: &mut View) {
    let frame = canvas::interact(
        ui,
        view,
        |_| scene.bounds,
        None,
        |point, scale| scene.hit(point, scale),
    );
    let painter = &frame.painter;
    let canvas = frame.rect;
    let visuals = ui.visuals();
    let text = visuals.strong_text_color();
    painter.rect_filled(canvas, 0, visuals.extreme_bg_color);

    // Parents precede descendants in the parsed order, so descendants stay visible.
    for (index, (node, world)) in chart.nodes.iter().zip(&scene.nodes).enumerate() {
        let Some(bounds) = world
            .map(|world| view.screen_rect(world, canvas))
            .filter(|bounds| canvas.intersects(*bounds))
        else {
            continue;
        };
        let selected = view.selected == Some(index);
        let border = match (selected, node.kind) {
            (true, _) => visuals.selection.stroke.color,
            (false, ChartNodeKind::GraphicalFunction) => visuals.hyperlink_color,
            _ => theme::OUTLINE,
        };
        let fill = if selected {
            visuals.selection.bg_fill
        } else {
            theme::OBJECT
        };
        let stroke = Stroke::new(if selected { 2.0 } else { 1.2 }, border);
        if node.kind == ChartNodeKind::Junction {
            painter.circle(
                bounds.center(),
                (bounds.width() / 2.0).max(2.0),
                fill,
                stroke,
            );
        } else {
            let radius = if node.kind == ChartNodeKind::State {
                5
            } else {
                0
            };
            painter.rect(bounds, radius, fill, stroke, StrokeKind::Inside);
        }
    }

    for (index, edge) in scene.edges.iter().enumerate() {
        let Some(edge) = edge else { continue };
        if !canvas.intersects(view.screen_rect(edge.bounds, canvas).expand(8.0)) {
            continue;
        }
        let color = if view.selected == Some(scene.nodes.len() + index) {
            visuals.hyperlink_color
        } else {
            theme::WIRE
        };
        let points: Vec<_> = edge
            .points
            .iter()
            .map(|&point| view.screen(point, canvas))
            .collect();
        let start = points[0];
        let end = points[points.len() - 1];
        if let Some(prior) = points
            .iter()
            .rev()
            .find(|point| end.distance(**point) > 0.01)
        {
            let direction = (end - *prior).normalized();
            let size = (view.scale() * 9.0).clamp(4.0, 10.0);
            let base = end - direction * size;
            let normal = vec2(-direction.y, direction.x) * (size * 0.42);
            painter.add(Shape::convex_polygon(
                vec![end, base + normal, base - normal],
                color,
                Stroke::NONE,
            ));
        }
        if edge.default {
            painter.circle_filled(start, 3.0, color);
        }
        painter.line(points, Stroke::new(1.2, color));
    }

    if view.scale() >= 0.28 {
        for (node, world) in chart.nodes.iter().zip(&scene.nodes) {
            if let (Some(label), Some(bounds)) = (
                &node.label,
                world.map(|world| view.screen_rect(world, canvas)),
            ) && canvas.intersects(bounds)
            {
                let font = font_size(node.properties.get("fontSize"), view.scale());
                painter.with_clip_rect(canvas.intersect(bounds)).text(
                    bounds.min + vec2(4.0, 3.0),
                    Align2::LEFT_TOP,
                    label,
                    font,
                    text,
                );
            }
        }
        for (transition, edge) in chart.transitions.iter().zip(&scene.edges) {
            let Some(edge) = edge else { continue };
            let Some(label) = transition
                .label
                .as_deref()
                .filter(|label| !label.is_empty())
            else {
                continue;
            };
            let font = font_size(transition.properties.get("fontSize"), view.scale());
            let at = edge
                .label
                .map(|rect| rect.min)
                .unwrap_or(edge.points[edge.points.len() / 2]);
            let at = view.screen(at, canvas);
            if canvas.expand(120.0).contains(at) {
                painter.text(at, Align2::LEFT_TOP, label, font, text);
            }
        }
    }
    if scene.bounds == Rect::NOTHING {
        painter.text(
            canvas.center(),
            Align2::CENTER_CENTER,
            "No chart geometry available",
            FontId::proportional(14.0),
            theme::MUTED,
        );
    }
}

fn font_size(saved: Option<&String>, scale: f32) -> FontId {
    let size = saved
        .and_then(|value| value.trim().parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(12.0);
    FontId::proportional((size * scale).clamp(6.0, 36.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::pos2;

    #[test]
    fn transition_route_uses_saved_endpoints_and_midpoint() {
        let endpoint = |target, x, y| ChartEndpoint {
            target,
            intersection: Some([0.0, 0.0, 0.0, 0.0, x, y, 0.0, 0.0]),
        };
        let mut transition = ChartTransition {
            ssid: "8".into(),
            parent: None,
            label: None,
            source: endpoint(ChartEndpointTarget::Default, 10.0, 20.0),
            destination: endpoint(
                ChartEndpointTarget::Node {
                    ssid: "1".into(),
                    index: Some(0),
                },
                50.0,
                20.0,
            ),
            midpoint: Some([30.0, 40.0]),
            label_bounds: None,
            execution_order: None,
            properties: Default::default(),
            slide_properties: Default::default(),
        };
        let edge = Edge::new(&transition).unwrap();
        assert!(edge.default);
        assert_eq!(edge.points.first(), Some(&pos2(10.0, 20.0)));
        assert_eq!(edge.points.get(8), Some(&pos2(30.0, 40.0)));
        assert_eq!(edge.points.last(), Some(&pos2(50.0, 20.0)));
        transition.destination.target = ChartEndpointTarget::Node {
            ssid: "1".into(),
            index: None,
        };
        assert!(Edge::new(&transition).is_none());
    }
}
