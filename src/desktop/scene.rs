use std::collections::BTreeMap;

use eframe::egui::{Pos2, Rect, pos2, vec2};
use slx_viewer::model::{Block, Bounds, Endpoint, Port, Segment, System};

pub struct Scene {
    pub blocks: Vec<Option<Rect>>,
    pub wires: Vec<Vec<Pos2>>,
    pub ports: Vec<Pos2>,
    pub labels: Vec<(Pos2, String)>,
    pub operators: Vec<(Pos2, String)>,
    pub bounds: Option<Rect>,
}

impl Scene {
    pub fn new(system: &System) -> Self {
        let block_rects: Vec<_> = system
            .blocks
            .iter()
            .map(|block| block.bounds.and_then(rect))
            .collect();
        let mut scene = Self {
            blocks: Vec::new(),
            wires: Vec::new(),
            ports: Vec::new(),
            labels: Vec::new(),
            operators: Vec::new(),
            bounds: None,
        };
        let counts: Vec<_> = system
            .blocks
            .iter()
            .map(|block| {
                let mut counts: BTreeMap<_, _> = block
                    .port_counts
                    .iter()
                    .map(|(kind, count)| (kind.as_str(), *count))
                    .collect();
                for port in &block.ports {
                    let count = counts.entry(&port.kind).or_default();
                    *count = (*count).max(port.index);
                }
                counts
            })
            .collect();
        for (index, (block, bounds)) in system.blocks.iter().zip(&block_rects).enumerate() {
            if let Some(bounds) = bounds {
                scene.include(*bounds);
                scene.add_label(bounds.center_bottom() + vec2(0.0, 3.0), &block.name);
                if matches!(block.block_type.as_str(), "Sum" | "Product")
                    && let Some(inputs) = block.properties.get("Inputs")
                {
                    let default = if block.block_type == "Sum" { '+' } else { '*' };
                    let signs: Vec<char> = if let Ok(count) = inputs.trim().parse::<usize>() {
                        vec![default; count.min(4096)]
                    } else {
                        inputs
                            .chars()
                            .filter(|c| matches!(c, '+' | '-' | '*' | '/'))
                            .take(4096)
                            .collect()
                    };
                    for (input, sign) in signs.iter().enumerate() {
                        let port = Port {
                            kind: "in".into(),
                            index: input as u32 + 1,
                        };
                        let mut input_counts = counts[index].clone();
                        input_counts.insert("in", signs.len() as u32);
                        let anchor = port_anchor(block, *bounds, &port, &input_counts);
                        let inward = (bounds.center() - anchor).normalized() * 7.0;
                        scene.operators.push((anchor + inward, sign.to_string()));
                    }
                }
                for port in &block.ports {
                    scene
                        .ports
                        .push(port_anchor(block, *bounds, port, &counts[index]));
                }
            }
        }
        let anchor = |endpoint: &Endpoint| {
            let index = endpoint.block?;
            Some(port_anchor(
                &system.blocks[index],
                block_rects[index]?,
                endpoint.port.as_ref()?,
                &counts[index],
            ))
        };
        for line in &system.connections {
            if let Some(start) = line.source.as_ref().and_then(anchor) {
                scene.add_segment(&line.route, start, &anchor);
            }
        }
        for annotation in &system.annotations {
            match *annotation.position.as_slice() {
                [x, y] => {
                    if let Some(anchor) = point([x, y]) {
                        scene.add_label(anchor, &annotation.text);
                    }
                }
                [left, top, right, bottom] => {
                    if let Some(bounds) = rect(Bounds {
                        min: [left, top],
                        max: [right, bottom],
                    }) {
                        scene.include(bounds);
                        scene.add_label(bounds.center_top(), &annotation.text);
                    }
                }
                _ => {}
            }
        }
        scene.blocks = block_rects;
        scene
    }

    fn include(&mut self, rect: Rect) {
        self.bounds = Some(self.bounds.map_or(rect, |bounds| bounds.union(rect)));
    }

    fn add_label(&mut self, at: Pos2, text: &str) {
        if !text.is_empty() {
            self.labels.push((at, text.into()));
        }
    }

    fn add_segment(
        &mut self,
        segment: &Segment,
        start: Pos2,
        anchor: &impl Fn(&Endpoint) -> Option<Pos2>,
    ) {
        let mut points = vec![start];
        let mut junction = start;
        for &offset in &segment.offsets {
            let Some(offset) = point(offset) else { return };
            let Some(next) = point([
                f64::from(junction.x + offset.x),
                f64::from(junction.y + offset.y),
            ]) else {
                return;
            };
            junction = next;
            points.push(junction);
        }
        if let Some(end) = segment.destination.as_ref().and_then(anchor) {
            if junction.x != end.x && junction.y != end.y {
                points.push(pos2(end.x, junction.y));
            }
            points.push(end);
        }
        if points.len() > 1 {
            for &point in &points {
                self.include(Rect::from_pos(point));
            }
            self.wires.push(points);
        }
        if let Some(label) = &segment.label {
            self.add_label(start.lerp(junction, 0.5), label);
        }
        for branch in &segment.branches {
            self.add_segment(branch, junction, anchor);
        }
    }
}

pub(super) fn point([x, y]: [f64; 2]) -> Option<Pos2> {
    const RANGE: std::ops::RangeInclusive<f64> = -10_000_000.0..=10_000_000.0;
    (RANGE.contains(&x) && RANGE.contains(&y)).then(|| pos2(x as f32, y as f32))
}

pub(super) fn rect(bounds: Bounds) -> Option<Rect> {
    Some(Rect::from_two_pos(point(bounds.min)?, point(bounds.max)?))
}

fn port_anchor(block: &Block, bounds: Rect, port: &Port, counts: &BTreeMap<&str, u32>) -> Pos2 {
    let count = counts
        .get(port.kind.as_str())
        .copied()
        .unwrap_or(port.index);
    let position = port.index as f32 / (count as f32 + 1.0);
    let (mut x, y) = match port.kind.as_str() {
        "in" | "lconn" => (0.0, position),
        "out" | "rconn" => (1.0, position),
        "state" => (position, 1.0),
        _ => (position, 0.0),
    };
    if block.mirrored {
        x = 1.0 - x;
    }
    let (x, y) = match block.rotation_degrees {
        90 => (1.0 - y, x),
        180 => (1.0 - x, 1.0 - y),
        270 => (y, 1.0 - x),
        _ => (x, y),
    };
    pos2(
        bounds.min.x + bounds.width() * x,
        bounds.min.y + bounds.height() * y,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use slx_viewer::{Limits, load};

    #[test]
    fn saved_branch_offsets_share_the_parent_junction() {
        let model = load(
            "tests/fixtures/dc_motor_speed_control.slx",
            &Limits::default(),
        )
        .unwrap();
        let scene = Scene::new(&model.systems[model.root]);
        for path in [
            &[pos2(1090.0, 425.0), pos2(1130.0, 425.0)][..],
            &[pos2(1130.0, 425.0), pos2(1132.0, 425.0)],
            &[
                pos2(1130.0, 425.0),
                pos2(1130.0, 570.0),
                pos2(1160.0, 570.0),
            ],
        ] {
            assert!(scene.wires.iter().any(|wire| wire == path), "{path:?}");
        }
    }
}
