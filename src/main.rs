use std::{env, process::ExitCode};

use slx_viewer::{Limits, load, model::stateflow::ChartNodeKind};

fn main() -> ExitCode {
    let mut args = env::args_os().skip(1);
    let Some(path) = args.next().filter(|_| args.next().is_none()) else {
        eprintln!("Usage: slx_viewer <model.slx>");
        return ExitCode::FAILURE;
    };
    if path == "--help" || path == "-h" {
        println!("Usage: slx_viewer <model.slx>");
        return ExitCode::SUCCESS;
    }
    match load(path, &Limits::default()) {
        Ok(model) => {
            println!("{}", model.name);
            if let Some(release) = model.release {
                println!("Saved release: {release}");
            }
            println!("Systems: {}", model.systems.len());
            for system in &model.systems {
                println!(
                    "  {}: {} blocks, {} signals, {} annotations [{}]",
                    system.name,
                    system.blocks.len(),
                    system.connections.len(),
                    system.annotations.len(),
                    system.source_part,
                );
            }
            if !model.charts.is_empty() {
                println!("Stateflow charts: {}", model.charts.len());
                for chart in &model.charts {
                    let count = |kind| chart.nodes.iter().filter(|node| node.kind == kind).count();
                    for instance in &chart.instances {
                        println!(
                            "  {} (chart {}): {} states, {} graphical functions, {} junctions, {} transitions",
                            instance.path.replace('\r', "\\r").replace('\n', "\\n"),
                            chart.source_id,
                            count(ChartNodeKind::State),
                            count(ChartNodeKind::GraphicalFunction),
                            count(ChartNodeKind::Junction),
                            chart.transitions.len(),
                        );
                    }
                }
            }
            for diagnostic in model.diagnostics {
                let object = diagnostic.object.as_deref().unwrap_or("diagram");
                eprintln!("{} ({object}): {}", diagnostic.part, diagnostic.message);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
