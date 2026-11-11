use std::collections::BTreeMap;

pub mod stateflow;

use stateflow::{ChartId, StateflowChart};

pub type SystemId = usize;
pub type BlockIndex = usize;
pub type Properties = BTreeMap<String, String>;

#[derive(Debug)]
pub struct Model {
    pub name: String,
    pub release: Option<String>,
    pub root: SystemId,
    pub systems: Vec<System>,
    pub charts: Vec<StateflowChart>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug)]
pub struct System {
    pub name: String,
    pub parent: Option<(SystemId, BlockIndex)>,
    pub source_part: String,
    pub blocks: Vec<Block>,
    pub connections: Vec<Connection>,
    pub annotations: Vec<Annotation>,
}

#[derive(Debug)]
pub struct Block {
    pub sid: String,
    pub name: String,
    pub block_type: String,
    pub bounds: Option<Bounds>,
    pub rotation_degrees: i32,
    pub mirrored: bool,
    pub port_counts: BTreeMap<String, u32>,
    pub ports: Vec<Port>,
    pub properties: Properties,
    pub child: Option<SystemId>,
    pub chart: Option<ChartId>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Port {
    pub kind: String,
    pub index: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: [f64; 2],
    pub max: [f64; 2],
}

#[derive(Debug)]
pub struct Endpoint {
    pub raw: String,
    pub block: Option<BlockIndex>,
    pub port: Option<Port>,
}

#[derive(Debug)]
pub struct Connection {
    pub source: Option<Endpoint>,
    pub route: Segment,
}

#[derive(Debug)]
pub struct Segment {
    pub destination: Option<Endpoint>,
    pub offsets: Vec<[f64; 2]>,
    pub label: Option<String>,
    pub branches: Vec<Segment>,
}

#[derive(Debug)]
pub struct Annotation {
    pub text: String,
    pub position: Vec<f64>,
    pub properties: Properties,
}

#[derive(Debug)]
pub struct Diagnostic {
    pub part: String,
    pub object: Option<String>,
    pub message: String,
}
