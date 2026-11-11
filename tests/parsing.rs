use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
};

use sha2::{Digest, Sha256};
use slx_viewer::{
    Limits, LoadError, load, load_cancellable,
    model::{
        Bounds,
        stateflow::{ChartEndpointTarget, ChartNodeGeometry},
    },
};
use tempfile::TempDir;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

const SAVED_MODEL: &str = "tests/fixtures/dc_motor_speed_control.slx";

#[test]
fn singleton_enable_connection_resolves() {
    let xml = r#"<ModelInformation><Model><System><Block BlockType="Constant" Name="enable" SID="1"/><Block BlockType="SubSystem" Name="controlled" SID="2"/><Line><P Name="Src">1#out:1</P><P Name="Dst">2#enable</P></Line></System></Model></ModelInformation>"#;
    let (_dir, path) = package(&[(DIAGRAM, xml)]);
    let model = load(path, &Limits::default()).unwrap();
    let endpoint = model.systems[model.root].connections[0]
        .route
        .destination
        .as_ref()
        .unwrap();
    assert_eq!(endpoint.block, Some(1));
    let port = endpoint.port.as_ref().unwrap();
    assert_eq!(port.kind, "enable");
    assert_eq!(port.index, 1);
    assert!(model.systems[model.root].blocks[1].ports.contains(port));
}
const DIAGRAM: &str = "simulink/blockdiagram.xml";
const SYSTEM_TYPE: &str = "http://schemas.mathworks.com/simulink/2010/relationships/system";
const STATEFLOW_TYPE: &str =
    "http://schemas.mathworks.com/simulinkModel/2014/relationships/stateflowMachine";
const DIAGRAM_RELS: &str = "simulink/_rels/blockdiagram.xml.rels";
const STATEFLOW: &str = "simulink/stateflow.xml";

#[test]
fn cancelled_load_stops_before_opening_the_model() {
    assert!(matches!(
        load_cancellable(SAVED_MODEL, &Limits::default(), &AtomicBool::new(true)),
        Err(LoadError::Cancelled)
    ));
}

fn package(parts: &[(&str, &str)]) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("model.slx");
    let mut zip = ZipWriter::new(File::create(&path).unwrap());
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, xml) in parts {
        zip.start_file(*name, options).unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    (dir, path)
}

fn hash(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}

fn stateflow_relationship(target: &str) -> String {
    format!(
        r#"<Relationships><Relationship Id="stateflow" Type="{STATEFLOW_TYPE}" Target="{target}"/></Relationships>"#
    )
}

const CHART_DIAGRAM: &str = r#"<Model><System>
    <Block SID="1" Name="Chart" BlockType="SubSystem">
        <P Name="Position">[1, 2, 3, 4]</P><P Name="SFBlockType">Chart</P><System/>
    </Block>
</System></Model>"#;

#[test]
fn stateflow_scopes_objects_and_diagnoses_unresolved_references() {
    let diagram = CHART_DIAGRAM.replace(
        "</System></Model>",
        r#"
        <Block SID="2" Name="Another" BlockType="SubSystem">
            <P Name="Position">[1, 2, 3, 4]</P><P Name="SFBlockType">Chart</P>
        </Block>
        <Block SID="3" Name="Missing" BlockType="SubSystem">
            <P Name="Position">[1, 2, 3, 4]</P><P Name="SFBlockType">Chart</P>
        </Block></System></Model>"#,
    );
    let stateflow = r#"<Stateflow><machine id="1"><Children><chart id="2"><Children>
        <state SSID="10"><P Name="position">[10 20 30 40]</P></state>
        <junction SSID="11"><P Name="position">[20 30 4]</P></junction>
        <transition SSID="12"><src><P Name="SSID">10</P></src><dst><P Name="SSID">11</P></dst></transition>
        <transition SSID="13"><src><P Name="SSID">20</P></src><dst/></transition>
    </Children></chart><chart id="3"><Children>
        <state SSID="10"><P Name="position">[1 2 3 4]</P></state>
        <state SSID="20"><P Name="position">[1 2 3 4]</P></state>
        <transition SSID="12"><dst><P Name="SSID">10</P></dst></transition>
    </Children></chart></Children></machine>
    <instance><P Name="machine">1</P><P Name="chart">2</P><P Name="name">Chart</P></instance>
    <instance><P Name="machine">1</P><P Name="chart">3</P><P Name="name">Another</P></instance>
    <instance><P Name="machine">1</P><P Name="chart">9</P><P Name="name">Missing</P></instance>
    </Stateflow>"#;
    let rel = stateflow_relationship("stateflow.xml");
    let (_dir, path) = package(&[
        (DIAGRAM, &diagram),
        (DIAGRAM_RELS, &rel),
        (STATEFLOW, stateflow),
    ]);
    let model = load(path, &Limits::default()).unwrap();
    let blocks = &model.systems[model.root].blocks;
    assert_eq!(model.charts.len(), 2);
    let first = &model.charts[blocks[0].chart.unwrap()];
    assert_eq!(
        first.nodes[1].geometry,
        Some(ChartNodeGeometry::Junction {
            center: [20.0, 30.0],
            radius: 4.0
        })
    );
    assert_eq!(
        first.transitions[0].destination.target,
        ChartEndpointTarget::Node {
            ssid: "11".into(),
            index: Some(1)
        }
    );
    assert_eq!(
        first.transitions[1].source.target,
        ChartEndpointTarget::Node {
            ssid: "20".into(),
            index: None
        }
    );
    assert_eq!(
        first.transitions[1].destination.target,
        ChartEndpointTarget::Missing
    );
    assert_eq!(
        model.charts[blocks[1].chart.unwrap()].transitions[0]
            .source
            .target,
        ChartEndpointTarget::Default
    );
    assert!(blocks[2].chart.is_none());
    assert!(model.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("chart 9 in machine 1 is missing")
    }));
}

#[test]
fn stateflow_rejects_ambiguous_paths_and_bindings() {
    let template = r#"<Stateflow><machine id="1"><Children><chart id="2"/></Children></machine>
        <instance><P Name="machine">1</P><P Name="chart">2</P><P Name="name">Chart</P></instance>{extra}</Stateflow>"#;
    let extra = r#"<instance><P Name="machine">1</P><P Name="chart">9</P><P Name="name">Chart</P></instance>"#;
    let duplicate = CHART_DIAGRAM.replace(
        "</System></Model>",
        r#"
        <Block SID="2" Name="Chart" BlockType="SubSystem">
            <P Name="Position">[1, 2, 3, 4]</P><P Name="SFBlockType">Chart</P>
        </Block></System></Model>"#,
    );
    let rel = stateflow_relationship("stateflow.xml");
    for (diagram, extra, expected) in [
        (&*duplicate, "", "ambiguous Stateflow block path"),
        (CHART_DIAGRAM, extra, "multiple Stateflow instances"),
    ] {
        let stateflow = template.replace("{extra}", extra);
        let (_dir, path) = package(&[
            (DIAGRAM, diagram),
            (DIAGRAM_RELS, &rel),
            (STATEFLOW, &stateflow),
        ]);
        let model = load(path, &Limits::default()).unwrap();
        assert!(model.charts.is_empty());
        assert!(model.systems[model.root].blocks[0].child.is_some());
        assert!(
            model
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains(expected))
        );
    }
}

#[test]
fn optional_stateflow_data_respects_package_limits_and_preserves_simulink_on_format_errors() {
    let rel = stateflow_relationship("stateflow.xml");
    let (_dir, path) = package(&[(DIAGRAM, CHART_DIAGRAM), (DIAGRAM_RELS, &rel)]);
    let model = load(path, &Limits::default()).unwrap();
    assert_eq!(model.systems.len(), 2);
    assert!(model.charts.is_empty());
    assert!(
        model
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.part == STATEFLOW
                && diagnostic.message.contains("missing package part"))
    );

    for xml in [
        "<Stateflow><machine></Stateflow>",
        r#"<!DOCTYPE Stateflow SYSTEM "https://example.invalid/chart.dtd"><Stateflow/>"#,
    ] {
        let (_dir, path) = package(&[
            (DIAGRAM, CHART_DIAGRAM),
            (DIAGRAM_RELS, &rel),
            (STATEFLOW, xml),
        ]);
        let model = load(path, &Limits::default()).unwrap();
        assert_eq!(model.systems.len(), 2);
        assert!(model.charts.is_empty());
        assert!(
            model
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.part == STATEFLOW
                    && diagnostic.message.contains("Stateflow data is unavailable"))
        );
    }

    let bad_rel = stateflow_relationship("../stateflow.xml");
    let (_dir, path) = package(&[(DIAGRAM, CHART_DIAGRAM), (DIAGRAM_RELS, &bad_rel)]);
    assert!(matches!(
        load(path, &Limits::default()),
        Err(LoadError::Path(_))
    ));

    let (_dir, path) = package(&[
        (DIAGRAM, CHART_DIAGRAM),
        (DIAGRAM_RELS, &rel),
        (
            STATEFLOW,
            r#"<Stateflow><machine id="1"><Children><chart id="2"/></Children></machine></Stateflow>"#,
        ),
    ]);
    let limits = Limits {
        xml_elements: 9,
        ..Limits::default()
    };
    assert!(matches!(
        load(path, &limits),
        Err(LoadError::Limit {
            resource: "XML element count",
            ..
        })
    ));
}

#[test]
fn saved_r2025b_model_preserves_observed_structure() {
    assert_eq!(
        hash(Path::new(SAVED_MODEL)),
        "f572a3a7bc3b8ff4a34eb09c8e39eb1464ea33f06a5c85521fb453345e8febf1"
    );
    let model = load(SAVED_MODEL, &Limits::default()).unwrap();
    assert_eq!(model.release.as_deref(), Some("R2025b"));
    assert_eq!(model.systems.len(), 1);
    assert!(model.charts.is_empty());
    assert!(model.diagnostics.is_empty(), "{:?}", model.diagnostics);
    let root = &model.systems[model.root];
    assert_eq!(root.source_part, "simulink/systems/system_root.xml");
    assert_eq!(
        (
            root.blocks.len(),
            root.connections.len(),
            root.annotations.len()
        ),
        (25, 22, 0)
    );
    let gain = root.blocks.iter().find(|block| block.sid == "16").unwrap();
    assert_eq!(
        (&*gain.name, &*gain.block_type, &*gain.properties["Gain"]),
        ("BackEMF", "Gain", "p.Ke")
    );
    assert_eq!(
        gain.bounds,
        Some(Bounds {
            min: [415.0, 435.0],
            max: [445.0, 465.0]
        })
    );
    let sum = root.blocks.iter().find(|block| block.sid == "12").unwrap();
    assert_eq!((sum.port_counts["in"], sum.port_counts["out"]), (3, 1));
    let current = root
        .connections
        .iter()
        .find(|line| {
            line.source
                .as_ref()
                .is_some_and(|endpoint| endpoint.raw == "14#out:1")
        })
        .unwrap();
    let source = current.source.as_ref().unwrap();
    assert_eq!(root.blocks[source.block.unwrap()].name, "ArmatureCurrent");
    assert_eq!(current.route.offsets, [[40.0, 0.0]]);
    let route = &current.route.branches;
    assert_eq!(route.len(), 2);
    assert_eq!(route[0].offsets, [[2.0, 0.0]]);
    assert_eq!(route[0].branches.len(), 2);
    assert_eq!(route[0].branches[1].offsets, [[9.0, 0.0], [0.0, 60.0]]);
    for (branch, name) in [
        (&route[0].branches[0], "Resistance"),
        (&route[0].branches[1], "MotorTorque"),
        (&route[1], "LogCurrent"),
    ] {
        let endpoint = branch.destination.as_ref().unwrap();
        assert_eq!(root.blocks[endpoint.block.unwrap()].name, name);
        let port = endpoint.port.as_ref().unwrap();
        assert_eq!((port.kind.as_str(), port.index), ("in", 1));
    }
}

#[test]
fn opens_read_only_file_through_cli_with_unicode_and_spaces() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("모델 copy.slx");
    fs::copy(SAVED_MODEL, &path).unwrap();
    let original_permissions = fs::metadata(&path).unwrap().permissions();
    let mut readonly = original_permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&path, readonly).unwrap();
    let before = hash(&path);
    let output = Command::new(env!("CARGO_BIN_EXE_slx_viewer"))
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary = String::from_utf8(output.stdout).unwrap();
    assert!(summary.contains("모델 copy"));
    assert!(summary.contains("25 blocks, 22 signals"));
    assert_eq!(before, hash(&path));
    fs::set_permissions(&path, original_permissions).unwrap();
    dir.close().unwrap();
}

#[test]
fn resolves_scoped_sids_in_embedded_and_referenced_systems() {
    let diagram = r#"<ModelInformation><Model Name="Example"><System>
        <Block SID="1" Name="Controller" BlockType="SubSystem">
          <P Name="Position">[1, 2, 3, 4]</P><System>
            <Block SID="1" Name="Filter" BlockType="SubSystem">
              <P Name="Position">[1, 2, 3, 4]</P><System Ref="filter"/>
            </Block>
            <Block SID="2" Name="Missing" BlockType="SubSystem">
              <P Name="Position">[1, 2, 3, 4]</P><System Ref="missing"/>
            </Block>
          </System>
        </Block>
        <Block SID="2" Name="Gain" BlockType="Gain"><P Name="Position">[1, 2, 3, 4]</P></Block>
        </System></Model></ModelInformation>"#;
    let relationships = format!(
        r#"<Relationships>
        <Relationship Id="filter" Type="{SYSTEM_TYPE}" Target="systems/filter.xml"/>
        <Relationship Id="missing" Type="{SYSTEM_TYPE}" Target="systems/missing.xml"/>
        </Relationships>"#
    );
    let filter = r#"<System>
        <Block SID="1" Name="Gain" BlockType="Gain">
          <P Name="Position">[2, 4, 6, 8]</P><P Name="BlockRotation">-90</P>
          <P Name="BlockMirror">on</P><P Name="Gain">Kp</P>
          <P Name="MaskInitialization"><![CDATA[system('never run');]]></P>
        </Block>
        <Line><P Name="Src">1#out:1</P><P Name="Dst">99#in:1</P></Line>
        <Annotation SID="2"><P Name="Name">유니코드 &amp; &#x2603;</P><P Name="Position">[1, 2]</P></Annotation>
        </System>"#;
    let package_relationships = r#"<Relationships>
        <Relationship Id="blockDiagram" Type="http://schemas.mathworks.com/simulink/2010/relationships/blockDiagram" Target="/diagram/main.xml"/>
        </Relationships>"#;
    let (_dir, path) = package(&[
        ("_rels/.rels", package_relationships),
        ("diagram/main.xml", diagram),
        ("diagram/_rels/main.xml.rels", &relationships),
        ("diagram/systems/filter.xml", filter),
    ]);
    let model = load(path, &Limits::default()).unwrap();
    let root = &model.systems[model.root];
    let controller_id = root.blocks[0].child.unwrap();
    let controller = &model.systems[controller_id];
    assert_eq!(controller.parent, Some((model.root, 0)));
    assert!(controller.blocks[1].child.is_none());
    let filter = &model.systems[controller.blocks[0].child.unwrap()];
    assert_eq!(filter.parent, Some((controller_id, 0)));
    assert_eq!(filter.source_part, "diagram/systems/filter.xml");
    assert_eq!(filter.blocks[0].name, root.blocks[1].name);
    assert_eq!(filter.blocks[0].properties["Gain"], "Kp");
    assert_eq!(
        filter.blocks[0].properties["MaskInitialization"],
        "system('never run');"
    );
    assert_eq!(filter.blocks[0].rotation_degrees, 270);
    assert!(filter.blocks[0].mirrored);
    assert_eq!(filter.annotations[0].text, "유니코드 & ☃");
    assert_eq!(
        filter.connections[0].source.as_ref().unwrap().block,
        Some(0)
    );
    assert_eq!(
        filter.connections[0]
            .route
            .destination
            .as_ref()
            .unwrap()
            .block,
        None
    );
    assert!(
        model
            .diagnostics
            .iter()
            .any(|d| d.message.contains("missing system part"))
    );
    assert!(
        model
            .diagnostics
            .iter()
            .any(|d| d.message.contains("99#in:1"))
    );
}

#[test]
fn rejects_invalid_zip_xml_and_root_parts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("corrupt.slx");
    fs::write(&path, b"invalid").unwrap();
    assert!(matches!(
        load(path, &Limits::default()),
        Err(LoadError::Zip(_))
    ));
    let (_dir, path) = package(&[("other.xml", "<Model/>")]);
    assert!(matches!(
        load(path, &Limits::default()),
        Err(LoadError::Missing(_))
    ));
    for document in [
        "<Model><System></Model>",
        "<Model><System>",
        "<Model><System/></Model><Other/>",
        "<Model/>",
        "<Model><System/>&custom;</Model>",
        r#"<!DOCTYPE Model SYSTEM "https://example.invalid/forbidden.dtd"><Model><System/></Model>"#,
    ] {
        let (_dir, path) = package(&[(DIAGRAM, document)]);
        assert!(
            matches!(
                load(path, &Limits::default()),
                Err(LoadError::Format { .. })
            ),
            "{document}"
        );
    }
}

#[test]
fn rejects_traversal_external_and_cyclic_system_references() {
    let root = r#"<Model><System Ref="root"/></Model>"#;
    let rels = "simulink/_rels/blockdiagram.xml.rels";
    for target in [
        "../system.xml",
        "%2e%2e/system.xml",
        "//example.invalid/system.xml",
    ] {
        let xml = format!(
            r#"<Relationships><Relationship Id="root" Type="{SYSTEM_TYPE}" Target="{target}"/></Relationships>"#
        );
        let (_dir, path) = package(&[(DIAGRAM, root), (rels, &xml)]);
        assert!(matches!(
            load(path, &Limits::default()),
            Err(LoadError::Path(_))
        ));
    }
    let external = format!(
        r#"<Relationships><Relationship Id="root" Type="{SYSTEM_TYPE}" Target="https://example.invalid/system.xml" TargetMode="External"/></Relationships>"#
    );
    let (_dir, path) = package(&[(DIAGRAM, root), (rels, &external)]);
    assert!(
        load(path, &Limits::default())
            .unwrap_err()
            .to_string()
            .contains("external relationship")
    );

    let cyclic = format!(
        r#"<Relationships><Relationship Id="root" Type="{SYSTEM_TYPE}" Target="blockdiagram.xml"/></Relationships>"#
    );
    let (_dir, path) = package(&[(DIAGRAM, root), (rels, &cyclic)]);
    assert!(
        load(path, &Limits::default())
            .unwrap_err()
            .to_string()
            .contains("cyclic system")
    );
}

#[test]
fn applies_limits_at_the_boundary_and_one_below() {
    let document = "<Model><System/></Model>";
    let (_dir, path) = package(&[(DIAGRAM, document)]);
    let exact = Limits {
        archive_bytes: fs::metadata(&path).unwrap().len(),
        entries: 1,
        expanded_bytes: document.len() as u64,
        part_bytes: document.len() as u64,
        xml_depth: 2,
        xml_elements: 2,
    };
    load(&path, &exact).unwrap();
    for (limits, expected) in [
        (
            Limits {
                archive_bytes: exact.archive_bytes - 1,
                ..exact.clone()
            },
            "compressed archive size",
        ),
        (
            Limits {
                entries: 0,
                ..exact.clone()
            },
            "ZIP entry count",
        ),
        (
            Limits {
                expanded_bytes: exact.expanded_bytes - 1,
                ..exact.clone()
            },
            "expanded archive size",
        ),
        (
            Limits {
                part_bytes: exact.part_bytes - 1,
                ..exact.clone()
            },
            "expanded part size",
        ),
        (
            Limits {
                xml_depth: 1,
                ..exact.clone()
            },
            "XML depth",
        ),
        (
            Limits {
                xml_elements: 1,
                ..exact
            },
            "XML element count",
        ),
    ] {
        assert!(
            matches!(load(&path, &limits), Err(LoadError::Limit { resource, .. }) if resource == expected),
            "{expected}"
        );
    }
}
