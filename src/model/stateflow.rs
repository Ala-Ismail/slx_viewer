//! Saved Stateflow chart data for read-only inspection.

use super::{BlockIndex, Bounds, Properties, SystemId};

pub type ChartId = usize;
pub type ChartNodeIndex = usize;

#[derive(Debug)]
pub struct StateflowChart {
    pub machine_id: String,
    pub source_id: String,
    pub name: Option<String>,
    pub source_part: String,
    pub properties: Properties,
    pub instances: Vec<ChartInstance>,
    pub nodes: Vec<ChartNode>,
    pub transitions: Vec<ChartTransition>,
}

#[derive(Debug)]
pub struct ChartInstance {
    pub path: String,
    pub owner: (SystemId, BlockIndex),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartNodeKind {
    State,
    GraphicalFunction,
    Junction,
}

#[derive(Debug)]
pub struct ChartNode {
    pub ssid: String,
    pub kind: ChartNodeKind,
    pub parent: Option<ChartNodeIndex>,
    pub label: Option<String>,
    pub geometry: Option<ChartNodeGeometry>,
    pub properties: Properties,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChartNodeGeometry {
    Bounds(Bounds),
    Junction { center: [f64; 2], radius: f64 },
}

#[derive(Debug)]
pub struct ChartTransition {
    pub ssid: String,
    pub parent: Option<ChartNodeIndex>,
    pub label: Option<String>,
    pub source: ChartEndpoint,
    pub destination: ChartEndpoint,
    pub midpoint: Option<[f64; 2]>,
    pub label_bounds: Option<Bounds>,
    pub execution_order: Option<u32>,
    pub properties: Properties,
    pub slide_properties: Properties,
}

#[derive(Debug)]
pub struct ChartEndpoint {
    pub target: ChartEndpointTarget,
    pub intersection: Option<[f64; 8]>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ChartEndpointTarget {
    Default,
    Node {
        ssid: String,
        index: Option<ChartNodeIndex>,
    },
    Missing,
}
