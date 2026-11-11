use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, btree_map, hash_map::Entry},
    sync::atomic::AtomicBool,
};

use crate::{
    LoadError, check_cancelled,
    model::{
        BlockIndex, Bounds, Diagnostic, System, SystemId,
        stateflow::{
            ChartEndpoint, ChartEndpointTarget, ChartInstance, ChartNode, ChartNodeGeometry,
            ChartNodeIndex, ChartNodeKind, ChartTransition, StateflowChart,
        },
    },
    xml::Node,
};

use super::{properties, property, row_numbers};

type ChartKey<'a> = (&'a str, &'a str);
type Owner = (SystemId, BlockIndex);
type BlockNames<'a> = HashMap<Cow<'a, str>, Option<BlockIndex>>;

pub(super) fn parse(
    root: &Node,
    part: &str,
    root_system: SystemId,
    systems: &[System],
    diagnostics: &mut Vec<Diagnostic>,
    cancelled: &AtomicBool,
) -> Result<Vec<StateflowChart>, LoadError> {
    let mut reader = ChartReader {
        part,
        diagnostics,
        cancelled,
    };
    let mut machines = HashMap::new();
    for machine in root.children_named("machine") {
        check_cancelled(cancelled)?;
        match machine
            .attr("id")
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            Some(id) if !index_unique(&mut machines, id, Some(machine)) => {
                reader.warn(Some(id), "duplicate Stateflow machine ID");
            }
            Some(_) => {}
            None => reader.warn(None, "Stateflow machine has no ID"),
        }
    }
    let mut definitions = HashMap::new();
    for (machine_id, machine) in machines {
        let Some(machine) = machine else { continue };
        for chart in machine
            .children_named("Children")
            .flat_map(|node| node.children_named("chart"))
        {
            check_cancelled(cancelled)?;
            match chart.attr("id").map(str::trim).filter(|id| !id.is_empty()) {
                Some(id) if !index_unique(&mut definitions, (machine_id, id), Some(chart)) => {
                    reader.warn(
                        Some(id),
                        format!("duplicate Stateflow chart ID in machine {machine_id}"),
                    );
                }
                Some(_) => {}
                None => reader.warn(Some(machine_id), "Stateflow chart has no ID"),
            }
        }
    }

    let mut names = Vec::with_capacity(systems.len());
    for system in systems {
        check_cancelled(cancelled)?;
        let mut blocks = HashMap::with_capacity(system.blocks.len());
        for (index, block) in system.blocks.iter().enumerate() {
            check_cancelled(cancelled)?;
            index_unique(&mut blocks, path_component(&block.name), Some(index));
        }
        names.push(blocks);
    }

    let mut bindings = BTreeMap::new();
    for instance in root.children_named("instance") {
        check_cancelled(cancelled)?;
        let (Some(path), Some(machine), Some(chart)) = (
            property(instance, "name").filter(|value| !value.is_empty()),
            property(instance, "machine")
                .map(str::trim)
                .filter(|value| !value.is_empty()),
            property(instance, "chart")
                .map(str::trim)
                .filter(|value| !value.is_empty()),
        ) else {
            reader.warn(
                None,
                "Stateflow instance requires a path, machine ID, and chart ID",
            );
            continue;
        };
        let owner = match resolve_owner(path, root_system, systems, &names) {
            Ok(owner) => owner,
            Err(message) => {
                reader.warn(Some(path), message);
                continue;
            }
        };
        match bindings.entry(owner) {
            btree_map::Entry::Vacant(entry) => {
                entry.insert(Some(((machine, chart), path)));
            }
            btree_map::Entry::Occupied(mut entry) => {
                entry.insert(None);
                reader.warn(
                    Some(path),
                    "multiple Stateflow instances point to this block",
                );
            }
        }
    }

    let mut parsed = HashMap::<ChartKey<'_>, usize>::new();
    let mut charts = Vec::new();
    for (owner @ (system, block), binding) in bindings {
        check_cancelled(cancelled)?;
        let Some((key @ (machine, chart), path)) = binding else {
            continue;
        };
        let Some(Some(definition)) = definitions.get(&key) else {
            reader.warn(
                Some(path),
                format!("Stateflow chart {chart} in machine {machine} is missing or ambiguous"),
            );
            continue;
        };
        if let Some(kind) = property(definition, "type")
            .map(str::trim)
            .filter(|kind| !kind.is_empty())
        {
            reader.warn(
                Some(path),
                format!("unsupported Stateflow chart type {kind}"),
            );
            continue;
        }
        if systems[system].blocks[block]
            .properties
            .get("SFBlockType")
            .is_none_or(|kind| kind != "Chart")
        {
            reader.warn(
                Some(path),
                "Stateflow instance does not point to a block marked as a chart",
            );
            continue;
        }
        let chart_id = match parsed.entry(key) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let chart_id = charts.len();
                charts.push(reader.chart(key, definition)?);
                *entry.insert(chart_id)
            }
        };
        charts[chart_id].instances.push(ChartInstance {
            path: path.into(),
            owner,
        });
    }
    Ok(charts)
}

fn index_unique<K: std::hash::Hash + Eq, V>(
    index: &mut HashMap<K, Option<V>>,
    key: K,
    value: Option<V>,
) -> bool {
    match index.entry(key) {
        Entry::Vacant(entry) => {
            entry.insert(value);
            true
        }
        Entry::Occupied(mut entry) => {
            entry.insert(None);
            false
        }
    }
}

fn path_component(name: &str) -> Cow<'_, str> {
    if name.contains("\\n") || name.contains("\r\n") {
        Cow::Owned(
            name.replace("\\r\\n", "\n")
                .replace("\\n", "\n")
                .replace("\r\n", "\n"),
        )
    } else {
        Cow::Borrowed(name)
    }
}

fn resolve_owner(
    path: &str,
    mut system: SystemId,
    systems: &[System],
    names: &[BlockNames<'_>],
) -> Result<Owner, &'static str> {
    let mut components = path.split('/').peekable();
    while let Some(component) = components.next() {
        let block = match names[system].get(&path_component(component)) {
            Some(Some(index)) => *index,
            Some(None) => return Err("ambiguous Stateflow block path"),
            None => return Err("Stateflow block path was not found"),
        };
        if components.peek().is_none() {
            return Ok((system, block));
        }
        system = systems[system].blocks[block]
            .child
            .ok_or("Stateflow block path crosses an unavailable subsystem")?;
    }
    Err("missing Stateflow block path")
}

struct ChartReader<'a> {
    part: &'a str,
    diagnostics: &'a mut Vec<Diagnostic>,
    cancelled: &'a AtomicBool,
}

#[derive(Default)]
struct ChartObjects<'a> {
    nodes: Vec<ChartNode>,
    transitions: Vec<(&'a Node, Option<ChartNodeIndex>)>,
}

impl ChartReader<'_> {
    fn warn(&mut self, object: Option<&str>, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            part: self.part.into(),
            object: object.map(str::to_owned),
            message: message.into(),
        });
    }

    fn chart(
        &mut self,
        (machine_id, id): ChartKey<'_>,
        chart: &Node,
    ) -> Result<StateflowChart, LoadError> {
        let mut objects = ChartObjects::default();
        self.objects(chart, id, None, &mut objects)?;
        // All saved diagram objects share the chart's SSID namespace.
        let mut ids = HashMap::new();
        for (index, node) in objects.nodes.iter().enumerate() {
            check_cancelled(self.cancelled)?;
            if !index_unique(&mut ids, node.ssid.as_str(), Some(index)) {
                self.warn(Some(id), format!("duplicate Stateflow SSID {}", node.ssid));
            }
        }
        let mut pending = Vec::new();
        for (node, parent) in objects.transitions {
            check_cancelled(self.cancelled)?;
            let Some(ssid) = self.ssid(node, id) else {
                continue;
            };
            if !index_unique(&mut ids, ssid, None) {
                self.warn(Some(id), format!("duplicate Stateflow SSID {ssid}"));
            }
            pending.push((node, parent, ssid));
        }
        let mut transitions = Vec::with_capacity(pending.len());
        for (node, parent, ssid) in pending {
            check_cancelled(self.cancelled)?;
            transitions.push(self.transition(node, id, ssid, parent, &ids));
        }
        Ok(StateflowChart {
            machine_id: machine_id.into(),
            source_id: id.into(),
            name: property(chart, "name").map(str::to_owned),
            source_part: self.part.into(),
            properties: properties(chart),
            instances: Vec::new(),
            nodes: objects.nodes,
            transitions,
        })
    }

    fn objects<'a>(
        &mut self,
        container: &'a Node,
        chart: &str,
        parent: Option<ChartNodeIndex>,
        objects: &mut ChartObjects<'a>,
    ) -> Result<(), LoadError> {
        for node in container
            .children_named("Children")
            .flat_map(|children| &children.children)
        {
            check_cancelled(self.cancelled)?;
            let kind = match node.name.as_str() {
                "state" if property(node, "type") == Some("FUNC_STATE") => {
                    ChartNodeKind::GraphicalFunction
                }
                "state" => ChartNodeKind::State,
                "junction" => ChartNodeKind::Junction,
                "transition" => {
                    objects.transitions.push((node, parent));
                    continue;
                }
                _ => continue,
            };
            let Some(ssid) = self.ssid(node, chart) else {
                continue;
            };
            let saved = property(node, "position");
            let geometry = if kind == ChartNodeKind::Junction {
                saved.and_then(vector).and_then(|[x, y, radius]| {
                    (radius >= 0.0).then_some(ChartNodeGeometry::Junction {
                        center: [x, y],
                        radius,
                    })
                })
            } else {
                saved.and_then(bounds).map(ChartNodeGeometry::Bounds)
            };
            if geometry.is_none() {
                self.warn(
                    Some(chart),
                    format!("Stateflow object {ssid} has missing or invalid position"),
                );
            }
            let index = objects.nodes.len();
            objects.nodes.push(ChartNode {
                ssid: ssid.into(),
                kind,
                parent,
                label: property(node, "labelString").map(str::to_owned),
                geometry,
                properties: properties(node),
            });
            self.objects(node, chart, Some(index), objects)?;
        }
        Ok(())
    }

    fn ssid<'a>(&mut self, node: &'a Node, chart: &str) -> Option<&'a str> {
        let id = node.attr("SSID").map(str::trim).filter(|id| !id.is_empty());
        if id.is_none() {
            self.warn(Some(chart), format!("Stateflow {} has no SSID", node.name));
        }
        id
    }

    fn transition(
        &mut self,
        node: &Node,
        chart: &str,
        ssid: &str,
        parent: Option<ChartNodeIndex>,
        ids: &HashMap<&str, Option<ChartNodeIndex>>,
    ) -> ChartTransition {
        let source = self.endpoint(node, chart, ssid, "src", ids);
        let destination = self.endpoint(node, chart, ssid, "dst", ids);
        let midpoint = self.saved(node, chart, ssid, "midPoint", vector);
        let label_bounds = self.saved(node, chart, ssid, "labelPosition", bounds);
        let execution_order = self.saved(node, chart, ssid, "executionOrder", |value| {
            value.trim().parse().ok()
        });
        ChartTransition {
            ssid: ssid.into(),
            parent,
            label: property(node, "labelString").map(str::to_owned),
            source,
            destination,
            midpoint,
            label_bounds,
            execution_order,
            properties: properties(node),
            slide_properties: node
                .children_named("slide")
                .next()
                .map(properties)
                .unwrap_or_default(),
        }
    }

    fn endpoint(
        &mut self,
        transition: &Node,
        chart: &str,
        ssid: &str,
        name: &str,
        ids: &HashMap<&str, Option<ChartNodeIndex>>,
    ) -> ChartEndpoint {
        let mut endpoints = transition.children_named(name);
        let node = endpoints.next();
        if endpoints.next().is_some() {
            self.warn(
                Some(chart),
                format!("Stateflow transition {ssid} has multiple {name} endpoints"),
            );
            return ChartEndpoint {
                target: ChartEndpointTarget::Missing,
                intersection: None,
            };
        }
        let reference = node.and_then(|node| property(node, "SSID")).map(str::trim);
        let target = match reference {
            Some(id) if !id.is_empty() => {
                let index = ids.get(id).copied().flatten();
                if index.is_none() {
                    self.warn(
                        Some(chart),
                        format!("Stateflow transition {ssid} has unresolved {name} SSID {id}"),
                    );
                }
                ChartEndpointTarget::Node {
                    ssid: id.into(),
                    index,
                }
            }
            None if name == "src" => ChartEndpointTarget::Default,
            _ => {
                self.warn(
                    Some(chart),
                    format!("Stateflow transition {ssid} has missing {name} SSID"),
                );
                ChartEndpointTarget::Missing
            }
        };
        let intersection =
            node.and_then(|node| self.saved(node, chart, ssid, "intersection", vector));
        ChartEndpoint {
            target,
            intersection,
        }
    }

    fn saved<T>(
        &mut self,
        node: &Node,
        chart: &str,
        ssid: &str,
        name: &str,
        parse: impl FnOnce(&str) -> Option<T>,
    ) -> Option<T> {
        let raw = property(node, name)?;
        let value = parse(raw);
        if value.is_none() {
            self.warn(
                Some(chart),
                format!("Stateflow transition {ssid} has invalid {name}"),
            );
        }
        value
    }
}

fn vector<const N: usize>(value: &str) -> Option<[f64; N]> {
    let mut numbers = row_numbers(value.trim().strip_prefix('[')?.strip_suffix(']')?);
    let mut result = [0.0; N];
    for item in &mut result {
        *item = numbers.next()??;
    }
    numbers.next().is_none().then_some(result)
}

fn bounds(value: &str) -> Option<Bounds> {
    let [x, y, width, height] = vector(value)?;
    let max = [x + width, y + height];
    (width >= 0.0 && height >= 0.0 && max.iter().all(|value| value.is_finite()))
        .then_some(Bounds { min: [x, y], max })
}
