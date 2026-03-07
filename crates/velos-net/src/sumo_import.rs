//! SUMO `.net.xml` network file importer.
//!
//! Parses a SUMO network file using streaming XML (quick-xml) and produces
//! a [`RoadGraph`] along with signal plans and a list of warnings for
//! unmapped attributes. Internal edges (prefixed with `:`) are filtered out.

use std::collections::HashMap;
use std::path::Path;

use petgraph::graph::{DiGraph, NodeIndex};
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use crate::error::NetError;
use crate::graph::{RoadClass, RoadEdge, RoadGraph, RoadNode};

/// A signal plan imported from SUMO `<tlLogic>`, with the junction ID it was
/// defined on.
#[derive(Debug, Clone)]
pub struct SumoSignalPlan {
    /// The `id` attribute of the `<tlLogic>` element.
    pub junction_id: String,
    /// The converted signal plan.
    pub plan: velos_signal::plan::SignalPlan,
}

/// Known attributes for `<location>` element.
const KNOWN_LOCATION_ATTRS: &[&str] = &[
    "netOffset",
    "convBoundary",
    "origBoundary",
    "projParameter",
];

/// Import a SUMO `.net.xml` file and return the road graph, signal plans, and
/// a list of warnings for unmapped attributes.
///
/// # Errors
/// Returns [`NetError::Io`] if the file cannot be read, or
/// [`NetError::XmlParse`] if the XML is malformed.
pub fn import_sumo_net(
    path: &Path,
) -> Result<(RoadGraph, Vec<SumoSignalPlan>, Vec<String>), NetError> {
    let xml_bytes = std::fs::read(path)?;
    let mut reader = Reader::from_reader(xml_bytes.as_slice());
    reader.config_mut().trim_text(true);

    let mut warnings: Vec<String> = Vec::new();

    // Intermediate storage keyed by SUMO IDs.
    let mut edges: Vec<SumoEdge> = Vec::new();
    let mut junctions: HashMap<String, SumoJunction> = HashMap::new();
    let mut connections: Vec<SumoConnection> = Vec::new();
    let mut signals: Vec<SumoSignalPlan> = Vec::new();
    let mut roundabout_edges: Vec<String> = Vec::new();

    // Parsing state.
    let mut current_edge: Option<SumoEdge> = None;
    let mut current_tl: Option<TlLogicBuilder> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                handle_element(
                    e,
                    &mut current_edge,
                    &mut current_tl,
                    &mut junctions,
                    &mut connections,
                    &mut roundabout_edges,
                    &mut warnings,
                );
            }
            Ok(Event::Empty(ref e)) => {
                handle_element(
                    e,
                    &mut current_edge,
                    &mut current_tl,
                    &mut junctions,
                    &mut connections,
                    &mut roundabout_edges,
                    &mut warnings,
                );
                // Self-closing elements also act as End for container elements.
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                finalize_tag(
                    &tag,
                    &mut current_edge,
                    &mut current_tl,
                    &mut edges,
                    &mut signals,
                );
            }
            Ok(Event::End(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                finalize_tag(
                    &tag,
                    &mut current_edge,
                    &mut current_tl,
                    &mut edges,
                    &mut signals,
                );
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(NetError::XmlParse(format!(
                    "XML parse error at position {}: {}",
                    reader.error_position(),
                    e
                )));
            }
            _ => {}
        }
        buf.clear();
    }

    // Log roundabout info.
    if !roundabout_edges.is_empty() {
        warnings.push(format!(
            "roundabout detected with edges: {}",
            roundabout_edges.join(", ")
        ));
    }

    // Build the petgraph DiGraph.
    let graph = build_graph(&edges, &junctions, &connections, &mut warnings);

    Ok((graph, signals, warnings))
}

/// Handle end-of-element (or self-closing element) by finalizing the
/// current edge or traffic-light builder.
fn finalize_tag(
    tag: &str,
    current_edge: &mut Option<SumoEdge>,
    current_tl: &mut Option<TlLogicBuilder>,
    edges: &mut Vec<SumoEdge>,
    signals: &mut Vec<SumoSignalPlan>,
) {
    match tag {
        "edge" => {
            if let Some(edge) = current_edge.take() {
                if !edge.internal {
                    edges.push(edge);
                }
            }
        }
        "tlLogic" => {
            if let Some(tl) = current_tl.take()
                && let Some(plan) = tl.build()
            {
                signals.push(plan);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Event dispatchers
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn handle_element(
    e: &BytesStart<'_>,
    current_edge: &mut Option<SumoEdge>,
    current_tl: &mut Option<TlLogicBuilder>,
    junctions: &mut HashMap<String, SumoJunction>,
    connections: &mut Vec<SumoConnection>,
    roundabout_edges: &mut Vec<String>,
    warnings: &mut Vec<String>,
) {
    let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
    match tag.as_str() {
        "edge" => {
            *current_edge = parse_edge_start(e, warnings);
        }
        "lane" => {
            if let Some(edge) = current_edge {
                parse_lane(e, edge, warnings);
            }
        }
        "junction" => {
            if let Some((id, jn)) = parse_junction(e, warnings) {
                junctions.insert(id, jn);
            }
        }
        "connection" => {
            if let Some(conn) = parse_connection(e, warnings) {
                connections.push(conn);
            }
        }
        "tlLogic" => {
            *current_tl = parse_tl_logic_start(e, warnings);
        }
        "phase" => {
            if let Some(tl) = current_tl {
                parse_tl_phase(e, tl, warnings);
            }
        }
        "roundabout" => {
            parse_roundabout(e, roundabout_edges, warnings);
        }
        "location" => {
            check_unmapped_attrs(e, KNOWN_LOCATION_ATTRS, "location", warnings);
        }
        "net" | "param" => {
            // Top-level container, ignore.
        }
        _ => {
            // Unknown element -- not a warning (SUMO has many optional elements).
        }
    }
}

fn finalize_tag(
    tag: &str,
    current_edge: &mut Option<SumoEdge>,
    current_tl: &mut Option<TlLogicBuilder>,
    edges: &mut Vec<SumoEdge>,
    signals: &mut Vec<SumoSignalPlan>,
) {
    match tag {
        "edge" => {
            if let Some(edge) = current_edge.take()
                && !edge.internal
            {
                edges.push(edge);
            }
        }
        "tlLogic" => {
            if let Some(tl) = current_tl.take()
                && let Some(plan) = tl.build()
            {
                signals.push(plan);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Internal parsing types
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct SumoEdge {
    id: String,
    from: String,
    to: String,
    edge_type: Option<String>,
    internal: bool,
    lanes: Vec<SumoLane>,
}

#[derive(Debug)]
struct SumoLane {
    speed: f64,
    length: f64,
}

#[derive(Debug)]
struct SumoJunction {
    x: f64,
    y: f64,
}

#[derive(Debug)]
#[allow(dead_code)] // Fields stored for future connection-level graph wiring.
struct SumoConnection {
    from_edge: String,
    to_edge: String,
}

struct TlLogicBuilder {
    id: String,
    phases: Vec<velos_signal::plan::SignalPhase>,
}

impl TlLogicBuilder {
    fn build(self) -> Option<SumoSignalPlan> {
        if self.phases.is_empty() {
            return None;
        }
        let plan = velos_signal::plan::SignalPlan::new(self.phases);
        Some(SumoSignalPlan {
            junction_id: self.id,
            plan,
        })
    }
}

// ---------------------------------------------------------------------------
// Element parsers
// ---------------------------------------------------------------------------

fn parse_edge_start(e: &BytesStart<'_>, warnings: &mut Vec<String>) -> Option<SumoEdge> {
    let mut id = String::new();
    let mut from = String::new();
    let mut to = String::new();
    let mut edge_type = None;
    let mut function = None;

    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let val = String::from_utf8_lossy(&attr.value).to_string();
        match key.as_str() {
            "id" => id = val,
            "from" => from = val,
            "to" => to = val,
            "type" => edge_type = Some(val),
            "function" => function = Some(val),
            "priority" | "name" => { /* known, used for context but not mapped */ }
            _ => {
                warnings.push(format!(
                    "Unmapped attribute on <edge id=\"{}\">: {}=\"{}\"",
                    id, key, val
                ));
            }
        }
    }

    let internal = id.starts_with(':') || function.as_deref() == Some("internal");

    Some(SumoEdge {
        id,
        from,
        to,
        edge_type,
        internal,
        lanes: Vec::new(),
    })
}

fn parse_lane(e: &BytesStart<'_>, edge: &mut SumoEdge, warnings: &mut Vec<String>) {
    let mut speed = 0.0;
    let mut length = 0.0;

    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let val = String::from_utf8_lossy(&attr.value).to_string();
        match key.as_str() {
            "id" | "index" | "shape" => { /* known, not mapped to output */ }
            "speed" => speed = val.parse().unwrap_or(0.0),
            "length" => length = val.parse().unwrap_or(0.0),
            _ => {
                warnings.push(format!(
                    "Unmapped attribute on <lane> of edge \"{}\": {}=\"{}\"",
                    edge.id, key, val
                ));
            }
        }
    }

    edge.lanes.push(SumoLane { speed, length });
}

fn parse_junction(
    e: &BytesStart<'_>,
    warnings: &mut Vec<String>,
) -> Option<(String, SumoJunction)> {
    let mut id = String::new();
    let mut x = 0.0;
    let mut y = 0.0;

    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let val = String::from_utf8_lossy(&attr.value).to_string();
        match key.as_str() {
            "id" => id = val,
            "x" => x = val.parse().unwrap_or(0.0),
            "y" => y = val.parse().unwrap_or(0.0),
            "type" | "incLanes" | "intLanes" | "shape" => { /* known */ }
            _ => {
                warnings.push(format!(
                    "Unmapped attribute on <junction id=\"{}\">: {}=\"{}\"",
                    id, key, val
                ));
            }
        }
    }

    // Skip internal junctions (those with ":" prefix).
    if id.starts_with(':') {
        return None;
    }

    Some((id, SumoJunction { x, y }))
}

fn parse_connection(e: &BytesStart<'_>, warnings: &mut Vec<String>) -> Option<SumoConnection> {
    let mut from_edge = String::new();
    let mut to_edge = String::new();

    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let val = String::from_utf8_lossy(&attr.value).to_string();
        match key.as_str() {
            "from" => from_edge = val,
            "to" => to_edge = val,
            "fromLane" | "toLane" | "via" | "dir" | "state" | "tl" | "linkIndex" => {
                /* known */
            }
            _ => {
                warnings.push(format!(
                    "Unmapped attribute on <connection>: {}=\"{}\"",
                    key, val
                ));
            }
        }
    }

    if from_edge.is_empty() || to_edge.is_empty() {
        return None;
    }

    Some(SumoConnection {
        from_edge,
        to_edge,
    })
}

fn parse_tl_logic_start(e: &BytesStart<'_>, warnings: &mut Vec<String>) -> Option<TlLogicBuilder> {
    let mut id = String::new();

    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let val = String::from_utf8_lossy(&attr.value).to_string();
        match key.as_str() {
            "id" => id = val,
            "type" | "programID" | "offset" => { /* known */ }
            _ => {
                warnings.push(format!(
                    "Unmapped attribute on <tlLogic id=\"{}\">: {}=\"{}\"",
                    id, key, val
                ));
            }
        }
    }

    Some(TlLogicBuilder {
        id,
        phases: Vec::new(),
    })
}

fn parse_tl_phase(
    e: &BytesStart<'_>,
    tl: &mut TlLogicBuilder,
    warnings: &mut Vec<String>,
) {
    let mut duration = 0.0;
    let mut state = String::new();

    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let val = String::from_utf8_lossy(&attr.value).to_string();
        match key.as_str() {
            "duration" => duration = val.parse().unwrap_or(0.0),
            "state" => state = val,
            _ => {
                warnings.push(format!(
                    "Unmapped attribute on <phase> in tlLogic \"{}\": {}=\"{}\"",
                    tl.id, key, val
                ));
            }
        }
    }

    // SUMO state string: G=green, g=green-minor, y=yellow, r=red.
    // Count green approaches (indices of 'G' or 'g' characters).
    let approaches: Vec<usize> = state
        .chars()
        .enumerate()
        .filter(|(_, c)| *c == 'G' || *c == 'g')
        .map(|(i, _)| i)
        .collect();

    // Determine amber: SUMO uses separate phases for yellow, we store green_duration.
    // If state contains only 'y' or 'r', this is an amber-only phase.
    let is_amber = state.chars().all(|c| c == 'y' || c == 'r') && state.contains('y');

    if is_amber {
        // Merge amber duration into the preceding phase if possible.
        if let Some(last) = tl.phases.last_mut() {
            last.amber_duration = duration;
            return;
        }
    }

    tl.phases.push(velos_signal::plan::SignalPhase {
        green_duration: duration,
        amber_duration: 0.0,
        approaches,
    });
}

fn parse_roundabout(
    e: &BytesStart<'_>,
    roundabout_edges: &mut Vec<String>,
    warnings: &mut Vec<String>,
) {
    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let val = String::from_utf8_lossy(&attr.value).to_string();
        match key.as_str() {
            "edges" => {
                roundabout_edges.extend(val.split_whitespace().map(String::from));
            }
            "nodes" => { /* known */ }
            _ => {
                warnings.push(format!(
                    "Unmapped attribute on <roundabout>: {}=\"{}\"",
                    key, val
                ));
            }
        }
    }
}

fn check_unmapped_attrs(
    e: &BytesStart<'_>,
    known: &[&str],
    element_name: &str,
    warnings: &mut Vec<String>,
) {
    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        if !known.contains(&key.as_str()) {
            let val = String::from_utf8_lossy(&attr.value).to_string();
            warnings.push(format!(
                "Unmapped attribute on <{}>: {}=\"{}\"",
                element_name, key, val
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Graph construction
// ---------------------------------------------------------------------------

/// Map a SUMO edge `type` attribute to a [`RoadClass`].
fn map_road_class(sumo_type: Option<&str>) -> RoadClass {
    match sumo_type {
        Some(t) if t.contains("motorway") => RoadClass::Motorway,
        Some(t) if t.contains("trunk") => RoadClass::Trunk,
        Some(t) if t.contains("primary") => RoadClass::Primary,
        Some(t) if t.contains("secondary") => RoadClass::Secondary,
        Some(t) if t.contains("tertiary") => RoadClass::Tertiary,
        Some(t) if t.contains("residential") => RoadClass::Residential,
        Some(t) if t.contains("service") => RoadClass::Service,
        _ => RoadClass::Residential, // fallback
    }
}

fn build_graph(
    edges: &[SumoEdge],
    junctions: &HashMap<String, SumoJunction>,
    _connections: &[SumoConnection],
    warnings: &mut Vec<String>,
) -> RoadGraph {
    let mut graph = DiGraph::<RoadNode, RoadEdge>::new();
    let mut junction_idx: HashMap<&str, NodeIndex> = HashMap::new();

    // Create nodes for each junction.
    for (id, jn) in junctions {
        let idx = graph.add_node(RoadNode {
            pos: [jn.x, jn.y],
        });
        junction_idx.insert(id.as_str(), idx);
    }

    // Create edges.
    for edge in edges {
        let from = match junction_idx.get(edge.from.as_str()) {
            Some(&idx) => idx,
            None => {
                warnings.push(format!(
                    "Edge \"{}\" references unknown from-junction \"{}\"",
                    edge.id, edge.from
                ));
                continue;
            }
        };
        let to = match junction_idx.get(edge.to.as_str()) {
            Some(&idx) => idx,
            None => {
                warnings.push(format!(
                    "Edge \"{}\" references unknown to-junction \"{}\"",
                    edge.id, edge.to
                ));
                continue;
            }
        };

        let lane_count = edge.lanes.len().max(1) as u8;
        let speed_limit_mps = edge
            .lanes
            .iter()
            .map(|l| l.speed)
            .fold(0.0_f64, f64::max);
        let length_m = edge
            .lanes
            .iter()
            .map(|l| l.length)
            .fold(0.0_f64, f64::max);
        let road_class = map_road_class(edge.edge_type.as_deref());

        graph.add_edge(
            from,
            to,
            RoadEdge {
                length_m,
                speed_limit_mps,
                lane_count,
                oneway: true, // SUMO edges are always directional.
                road_class,
                geometry: Vec::new(), // Shape parsing deferred.
                motorbike_only: false,
                time_windows: None,
            },
        );
    }

    RoadGraph::new(graph)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_road_class_primary() {
        assert_eq!(map_road_class(Some("highway.primary")), RoadClass::Primary);
    }

    #[test]
    fn map_road_class_fallback() {
        assert_eq!(map_road_class(None), RoadClass::Residential);
    }

    #[test]
    fn internal_edge_detected_by_colon() {
        let mut warnings = Vec::new();
        let xml = br#"<edge id=":j1_0" function="internal"/>"#;
        let mut reader = Reader::from_reader(xml.as_slice());
        let mut buf = Vec::new();
        if let Ok(Event::Empty(ref e)) = reader.read_event_into(&mut buf) {
            let edge = parse_edge_start(e, &mut warnings);
            assert!(edge.unwrap().internal);
        }
    }
}
