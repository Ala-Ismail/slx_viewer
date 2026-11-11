use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    sync::atomic::AtomicBool,
};

mod stateflow;

use crate::{
    Limits, LoadError,
    archive::{Package, resolve_target},
    check_cancelled,
    model::{
        Annotation, Block, BlockIndex, Bounds, Connection, Diagnostic, Endpoint, Model, Port,
        Properties, Segment, System, SystemId, stateflow::StateflowChart,
    },
    xml::{Node, XmlBudget},
};

const DIAGRAM_REL: &str = "http://schemas.mathworks.com/simulink/2010/relationships/blockDiagram";
const SYSTEM_REL: &str = "http://schemas.mathworks.com/simulink/2010/relationships/system";
const RELEASE_REL: &str =
    "http://schemas.mathworks.com/package/2019/relationships/corePropertiesReleaseInfo";
const STATEFLOW_REL: &str =
    "http://schemas.mathworks.com/simulinkModel/2014/relationships/stateflowMachine";

pub(crate) fn parse<'a>(
    package: Package<'a>,
    name: &str,
    limits: &Limits,
    cancelled: &'a AtomicBool,
) -> Result<Model, LoadError> {
    let mut parser = Parser {
        package,
        cancelled,
        xml: XmlBudget::new(limits),
        relations: HashMap::new(),
        active_parts: HashSet::new(),
        systems: Vec::new(),
        diagnostics: Vec::new(),
    };
    let diagram = parser
        .target("", DIAGRAM_REL, None)?
        .unwrap_or_else(|| "simulink/blockdiagram.xml".into());
    let release_part = parser.target("", RELEASE_REL, None)?.or_else(|| {
        parser
            .package
            .contains("metadata/mwcorePropertiesReleaseInfo.xml")
            .then(|| "metadata/mwcorePropertiesReleaseInfo.xml".into())
    });
    let release = if let Some(part) = release_part {
        if parser.package.contains(&part) {
            parser
                .read(&part)?
                .children_named("release")
                .next()
                .map(|node| node.text.trim().to_owned())
        } else {
            parser.warn(&part, None, "release metadata part is missing");
            None
        }
    } else {
        None
    };
    let document = parser.read(&diagram)?;
    let model = if document.name == "ModelInformation" {
        document.children_named("Model").next()
    } else if document.name == "Model" {
        Some(&document)
    } else {
        None
    }
    .ok_or_else(|| LoadError::format(&diagram, "expected a saved Model element"))?;
    let root = single_child(model, "System", &diagram)?
        .ok_or_else(|| LoadError::format(&diagram, "missing root System element"))?;
    let name = model
        .attr("Name")
        .filter(|value| !value.is_empty())
        .unwrap_or(name)
        .to_owned();
    parser.active_parts.insert(diagram.clone());
    let root = parser
        .system(root, &diagram, &name, None, 0)?
        .ok_or_else(|| LoadError::format(&diagram, "root system is unavailable"))?;
    drop(document);
    let charts = parser.stateflow(&diagram, root)?;
    Ok(Model {
        name,
        release,
        root,
        systems: parser.systems,
        charts,
        diagnostics: parser.diagnostics,
    })
}

struct Relationship {
    id: String,
    kind: String,
    target: String,
    external: bool,
}

struct Parser<'a> {
    package: Package<'a>,
    cancelled: &'a AtomicBool,
    xml: XmlBudget,
    relations: HashMap<String, Vec<Relationship>>,
    active_parts: HashSet<String>,
    systems: Vec<System>,
    diagnostics: Vec<Diagnostic>,
}

impl Parser<'_> {
    fn stateflow(
        &mut self,
        diagram: &str,
        root: SystemId,
    ) -> Result<Vec<StateflowChart>, LoadError> {
        let loaded = self
            .target(diagram, STATEFLOW_REL, None)
            .and_then(|target| {
                target
                    .map(|part| self.read(&part).map(|node| (part, node)))
                    .transpose()
            });
        let mut charts = Vec::new();
        match loaded {
            Ok(Some((part, node))) if node.name == "Stateflow" => {
                charts = stateflow::parse(
                    &node,
                    &part,
                    root,
                    &self.systems,
                    &mut self.diagnostics,
                    self.cancelled,
                )?;
            }
            Ok(Some((part, _))) => self.warn(&part, None, "expected a Stateflow root element"),
            Ok(None) => {}
            Err(error @ (LoadError::Cancelled | LoadError::Limit { .. } | LoadError::Path(_))) => {
                return Err(error);
            }
            Err(error) => {
                let part = match &error {
                    LoadError::Format { part, .. }
                    | LoadError::Read { part, .. }
                    | LoadError::Missing(part) => part,
                    _ => diagram,
                };
                self.warn(
                    part,
                    None,
                    format!("Stateflow data is unavailable. {error}"),
                );
            }
        }
        for (chart_id, chart) in charts.iter().enumerate() {
            for instance in &chart.instances {
                let (system, block) = instance.owner;
                self.systems[system].blocks[block].chart = Some(chart_id);
            }
        }
        for system in &self.systems {
            check_cancelled(self.cancelled)?;
            for block in &system.blocks {
                if block.chart.is_none()
                    && block
                        .properties
                        .get("SFBlockType")
                        .is_some_and(|kind| kind == "Chart")
                {
                    self.diagnostics.push(Diagnostic {
                        part: system.source_part.clone(),
                        object: Some(block.sid.clone()),
                        message: "no saved Stateflow chart could be linked to this block".into(),
                    });
                }
            }
        }
        Ok(charts)
    }

    fn read(&mut self, part: &str) -> Result<Node, LoadError> {
        self.xml
            .parse(&self.package.read(part)?, part, self.cancelled)
    }

    fn warn(&mut self, part: &str, object: Option<&str>, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            part: part.into(),
            object: object.map(str::to_owned),
            message: message.into(),
        });
    }

    fn target(
        &mut self,
        owner: &str,
        kind: &str,
        id: Option<&str>,
    ) -> Result<Option<String>, LoadError> {
        if !self.relations.contains_key(owner) {
            let part = match owner.rsplit_once('/') {
                Some((folder, file)) => format!("{folder}/_rels/{file}.rels"),
                None => format!("_rels/{owner}.rels"),
            };
            let mut relations = Vec::new();
            if self.package.contains(&part) {
                let root = self.read(&part)?;
                if root.name != "Relationships" {
                    return Err(LoadError::format(&part, "expected a Relationships element"));
                }
                let mut ids = HashSet::new();
                for node in root.children_named("Relationship") {
                    let id = required_attr(node, "Id", &part)?;
                    if !ids.insert(id) {
                        return Err(LoadError::format(
                            &part,
                            format!("duplicate relationship ID {id}"),
                        ));
                    }
                    let target = required_attr(node, "Target", &part)?;
                    let external = match node.attr("TargetMode") {
                        None | Some("Internal") => false,
                        Some("External") => true,
                        _ => {
                            return Err(LoadError::format(
                                &part,
                                "invalid relationship TargetMode",
                            ));
                        }
                    };
                    relations.push(Relationship {
                        id: id.into(),
                        kind: required_attr(node, "Type", &part)?.into(),
                        target: if external {
                            target.into()
                        } else {
                            resolve_target(owner, target)?
                        },
                        external,
                    });
                }
            }
            self.relations.insert(owner.into(), relations);
        }
        let mut matches = self.relations[owner]
            .iter()
            .filter(|rel| rel.kind == kind && id.is_none_or(|id| rel.id == id));
        let Some(relation) = matches.next() else {
            return Ok(None);
        };
        if matches.next().is_some() {
            return Err(LoadError::format(
                owner,
                format!("ambiguous relationship of type {kind}"),
            ));
        }
        if relation.external {
            return Err(LoadError::format(
                owner,
                format!("external relationship {} is forbidden", relation.id),
            ));
        }
        Ok(Some(relation.target.clone()))
    }

    fn system(
        &mut self,
        node: &Node,
        part: &str,
        name: &str,
        parent: Option<(SystemId, BlockIndex)>,
        depth: usize,
    ) -> Result<Option<SystemId>, LoadError> {
        const MAX_SYSTEM_DEPTH: usize = 32;
        if depth >= MAX_SYSTEM_DEPTH {
            return Err(LoadError::limit("system hierarchy depth", MAX_SYSTEM_DEPTH));
        }
        let Some(reference) = node.attr("Ref") else {
            return self
                .inline_system(node, part, name, parent, depth)
                .map(Some);
        };
        if !node.children.is_empty() {
            return Err(LoadError::format(
                part,
                "referenced System also contains embedded content",
            ));
        }
        let Some(target) = self.target(part, SYSTEM_REL, Some(reference))? else {
            return self.unavailable_system(
                part,
                name,
                parent,
                format!("missing system relationship {reference}"),
            );
        };
        if !self.package.contains(&target) {
            return self.unavailable_system(
                part,
                name,
                parent,
                format!("missing system part {target}"),
            );
        }
        if !self.active_parts.insert(target.clone()) {
            return Err(LoadError::format(
                part,
                format!("cyclic system reference to {target}"),
            ));
        }
        let root = self.read(&target)?;
        if root.name != "System" || root.attr("Ref").is_some() {
            return Err(LoadError::format(&target, "expected a System definition"));
        }
        let result = self.inline_system(&root, &target, name, parent, depth);
        self.active_parts.remove(&target);
        result.map(Some)
    }

    fn unavailable_system(
        &mut self,
        part: &str,
        name: &str,
        parent: Option<(SystemId, BlockIndex)>,
        message: String,
    ) -> Result<Option<SystemId>, LoadError> {
        if parent.is_none() {
            return Err(LoadError::format(part, message));
        }
        self.warn(part, Some(name), message);
        Ok(None)
    }

    fn inline_system(
        &mut self,
        node: &Node,
        part: &str,
        name: &str,
        parent: Option<(SystemId, BlockIndex)>,
        depth: usize,
    ) -> Result<SystemId, LoadError> {
        let id = self.systems.len();
        self.systems.push(System {
            name: name.into(),
            parent,
            source_part: part.into(),
            blocks: Vec::new(),
            connections: Vec::new(),
            annotations: Vec::new(),
        });
        let mut blocks = Vec::new();
        let mut indices = HashMap::new();
        for child in node.children_named("Block") {
            check_cancelled(self.cancelled)?;
            let index = blocks.len();
            let sid = required_attr(child, "SID", part)?;
            if indices.insert(sid, index).is_some() {
                return Err(LoadError::format(
                    part,
                    format!("duplicate block SID {sid} in {name}"),
                ));
            }
            let mut block = self.block(child, part, sid)?;
            if let Some(interior) = single_child(child, "System", part)? {
                block.child =
                    self.system(interior, part, &block.name, Some((id, index)), depth + 1)?;
            } else if matches!(
                block.block_type.as_str(),
                "SubSystem" | "ModelReference" | "Reference" | "Stateflow"
            ) {
                self.warn(part, Some(sid), "no saved child diagram is available");
            }
            blocks.push(block);
        }

        let mut ports = vec![BTreeSet::new(); blocks.len()];
        let mut connections = Vec::new();
        for line in node.children_named("Line") {
            check_cancelled(self.cancelled)?;
            let source =
                property(line, "Src").map(|value| self.endpoint(value, part, &indices, &mut ports));
            connections.push(Connection {
                source,
                route: self.segment(line, part, &indices, &mut ports),
            });
        }
        for (block, ports) in blocks.iter_mut().zip(ports) {
            block.ports = ports.into_iter().collect();
        }

        let mut annotations = Vec::new();
        for annotation in node.children_named("Annotation") {
            check_cancelled(self.cancelled)?;
            let properties = properties(annotation);
            let position = properties
                .get("Position")
                .and_then(|value| numbers(value))
                .unwrap_or_default();
            if !matches!(position.len(), 2 | 4) {
                self.warn(
                    part,
                    annotation.attr("SID"),
                    "annotation has missing or unsupported position",
                );
            }
            let text = properties
                .get("Name")
                .or_else(|| properties.get("Text"))
                .cloned()
                .unwrap_or_default();
            annotations.push(Annotation {
                text,
                position,
                properties,
            });
        }
        let system = &mut self.systems[id];
        system.blocks = blocks;
        system.connections = connections;
        system.annotations = annotations;
        Ok(id)
    }

    fn block(&mut self, node: &Node, part: &str, sid: &str) -> Result<Block, LoadError> {
        let properties = properties(node);
        let bounds = match properties
            .get("Position")
            .and_then(|value| numbers(value))
            .as_deref()
        {
            Some(&[left, top, right, bottom]) if left <= right && top <= bottom => Some(Bounds {
                min: [left, top],
                max: [right, bottom],
            }),
            _ => {
                self.warn(part, Some(sid), "block has missing or unsupported position");
                None
            }
        };
        let orientation = properties
            .get("BlockRotation")
            .or_else(|| properties.get("BlockOrientation"));
        let rotation = match orientation.map(|value| value.trim()) {
            None | Some("right") => Some(0),
            Some("down") => Some(90),
            Some("left") => Some(180),
            Some("up") => Some(270),
            Some(value) => value.parse::<i32>().ok().filter(|value| value % 90 == 0),
        };
        if rotation.is_none() {
            self.warn(part, Some(sid), "block has unsupported rotation");
        }
        let mirrored = properties
            .get("BlockMirror")
            .is_some_and(|value| value.trim() == "on");
        let mut port_counts = BTreeMap::new();
        if let Some(counts) = single_child(node, "PortCounts", part)? {
            for (kind, count) in counts.attrs() {
                if let Ok(count) = count.parse() {
                    port_counts.insert(kind.into(), count);
                } else {
                    self.warn(part, Some(sid), format!("invalid saved {kind} port count"));
                }
            }
        } else if let Some(counts) = properties.get("Ports") {
            let kinds = [
                "in", "out", "enable", "trigger", "state", "lconn", "rconn", "ifaction",
            ];
            let parsed = numbers(counts).filter(|counts| {
                counts.len() <= kinds.len()
                    && counts
                        .iter()
                        .all(|n| n.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(n))
            });
            if let Some(counts) = parsed {
                port_counts.extend(
                    kinds
                        .into_iter()
                        .zip(counts)
                        .map(|(kind, count)| (kind.into(), count as u32)),
                );
            } else {
                self.warn(part, Some(sid), "invalid saved Ports vector");
            }
        }
        Ok(Block {
            sid: sid.into(),
            name: required_attr(node, "Name", part)?.into(),
            block_type: required_attr(node, "BlockType", part)?.into(),
            bounds,
            rotation_degrees: rotation.unwrap_or(0).rem_euclid(360),
            mirrored,
            port_counts,
            ports: Vec::new(),
            properties,
            child: None,
            chart: None,
        })
    }

    fn endpoint(
        &mut self,
        raw: &str,
        part: &str,
        indices: &HashMap<&str, BlockIndex>,
        ports: &mut [BTreeSet<Port>],
    ) -> Endpoint {
        let raw = raw.trim();
        let mut endpoint = Endpoint {
            raw: raw.into(),
            block: None,
            port: None,
        };
        if let Some((sid, address)) = raw.split_once('#') {
            endpoint.block = indices.get(sid).copied();
            let singleton = matches!(address, "enable" | "trigger" | "ifaction");
            let address_parts = address
                .split_once(':')
                .or_else(|| singleton.then_some((address, "1")));
            if let Some((kind, index)) = address_parts
                && let Ok(index) = index.parse::<u32>()
                && index > 0
                && !kind.is_empty()
                && kind.bytes().all(|byte| byte.is_ascii_alphabetic())
            {
                let port = Port {
                    kind: kind.into(),
                    index,
                };
                if let Some(block) = endpoint.block {
                    ports[block].insert(port.clone());
                }
                endpoint.port = Some(port);
            }
        }
        if endpoint.block.is_none() || endpoint.port.is_none() {
            self.warn(part, None, format!("unresolved signal endpoint {raw:?}"));
        }
        endpoint
    }

    fn segment(
        &mut self,
        node: &Node,
        part: &str,
        indices: &HashMap<&str, BlockIndex>,
        ports: &mut [BTreeSet<Port>],
    ) -> Segment {
        let mut offsets = Vec::new();
        if let Some(raw) = property(node, "Points") {
            if let Some(points) = route_offsets(raw) {
                offsets = points;
            } else {
                self.warn(part, None, "invalid signal route points");
            }
        }
        Segment {
            destination: property(node, "Dst")
                .map(|value| self.endpoint(value, part, indices, ports)),
            offsets,
            label: property(node, "Name").map(str::to_owned),
            branches: node
                .children_named("Branch")
                .map(|branch| self.segment(branch, part, indices, ports))
                .collect(),
        }
    }
}

fn single_child<'a>(node: &'a Node, name: &str, part: &str) -> Result<Option<&'a Node>, LoadError> {
    let mut matches = node.children.iter().filter(|node| node.name == name);
    let first = matches.next();
    if matches.next().is_some() {
        return Err(LoadError::format(
            part,
            format!("multiple {name} elements inside {}", node.name),
        ));
    }
    Ok(first)
}

fn required_attr<'a>(node: &'a Node, name: &str, part: &str) -> Result<&'a str, LoadError> {
    node.attr(name)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            LoadError::format(part, format!("{} requires a {name} attribute", node.name))
        })
}

fn property<'a>(node: &'a Node, name: &str) -> Option<&'a str> {
    node.children
        .iter()
        .rev()
        .find(|child| child.name == "P" && child.attr("Name") == Some(name))
        .map(|node| node.text.as_str())
}

fn properties(node: &Node) -> Properties {
    node.children_named("P")
        .filter_map(|child| Some((child.attr("Name")?.into(), child.text.clone())))
        .collect()
}

fn numbers(value: &str) -> Option<Vec<f64>> {
    let body = value.trim().strip_prefix('[')?.strip_suffix(']')?;
    if body.contains(';') {
        return None;
    }
    row_numbers(body).collect()
}

fn route_offsets(value: &str) -> Option<Vec<[f64; 2]>> {
    let body = value.trim().strip_prefix('[')?.strip_suffix(']')?;
    if body.trim().is_empty() {
        return Some(Vec::new());
    }
    body.split(';')
        .map(|row| {
            let mut numbers = row_numbers(row);
            let pair = [numbers.next()??, numbers.next()??];
            numbers.next().is_none().then_some(pair)
        })
        .collect()
}

fn row_numbers(value: &str) -> impl Iterator<Item = Option<f64>> + '_ {
    value
        .split(|c: char| c.is_ascii_whitespace() || c == ',')
        .filter(|item| !item.is_empty())
        .map(|item| item.parse::<f64>().ok().filter(|value| value.is_finite()))
}
