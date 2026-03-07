//! SUMO `.rou.xml` and `.trips.xml` demand file importer.
//!
//! Parses SUMO route/trip/flow definitions and produces vehicle and person
//! spawn requests. Supports vType definitions, vTypeDistribution, and
//! calibrator elements. Unmapped attributes always produce warnings.

use std::collections::HashMap;
use std::path::Path;

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use crate::error::NetError;

/// Car-following model type parsed from SUMO `carFollowModel` attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CarFollowModelType {
    /// Krauss (SUMO default).
    Krauss,
    /// Intelligent Driver Model.
    IDM,
    /// Any other model (name stored).
    Other(String),
}

/// Vehicle parameters parsed from a SUMO `<vType>` element.
#[derive(Debug, Clone)]
pub struct SumoVehicleParams {
    /// Maximum acceleration (m/s^2).
    pub accel: f64,
    /// Maximum deceleration (m/s^2).
    pub decel: f64,
    /// Maximum speed (m/s).
    pub max_speed: f64,
    /// Driver imperfection (Krauss sigma). `None` if not specified.
    pub sigma: Option<f64>,
    /// Minimum gap to leader (m).
    pub min_gap: f64,
    /// Vehicle length (m).
    pub length: f64,
    /// Car-following model.
    pub car_follow_model: CarFollowModelType,
}

impl Default for SumoVehicleParams {
    fn default() -> Self {
        Self {
            accel: 2.6,
            decel: 4.5,
            max_speed: 55.55,
            sigma: None,
            min_gap: 2.5,
            length: 5.0,
            car_follow_model: CarFollowModelType::Krauss,
        }
    }
}

/// A vehicle definition parsed from SUMO `<vehicle>`, `<trip>`, or `<flow>`.
#[derive(Debug, Clone)]
pub struct SumoVehicleDef {
    /// Vehicle ID.
    pub id: String,
    /// Vehicle type ID (references a parsed vType).
    pub vtype: String,
    /// Departure time in seconds.
    pub depart: f64,
    /// Route as a sequence of edge IDs.
    pub route: Vec<String>,
    /// Vehicle parameters (resolved from vType).
    pub params: SumoVehicleParams,
}

/// A stage in a person's plan.
#[derive(Debug, Clone)]
pub enum SumoStage {
    /// Walking between edges.
    Walk { from: String, to: String },
    /// Riding a vehicle.
    Ride {
        from: String,
        to: String,
        lines: String,
    },
}

/// A person definition parsed from SUMO `<person>`.
#[derive(Debug, Clone)]
pub struct SumoPersonDef {
    /// Person ID.
    pub id: String,
    /// Departure time in seconds.
    pub depart: f64,
    /// Ordered list of stages.
    pub stages: Vec<SumoStage>,
}

/// A vType entry with optional probability (for vTypeDistribution).
#[derive(Debug, Clone)]
struct VTypeEntry {
    params: SumoVehicleParams,
    #[allow(dead_code)] // Stored for future vTypeDistribution weighted sampling.
    probability: Option<f64>,
}

/// Result type for SUMO route import: (vehicles, persons, warnings).
pub type SumoRouteImportResult = (Vec<SumoVehicleDef>, Vec<SumoPersonDef>, Vec<String>);

/// Import a SUMO `.rou.xml` file and return vehicle definitions, person
/// definitions, and a list of warnings.
///
/// # Errors
/// Returns [`NetError::Io`] if the file cannot be read, or
/// [`NetError::XmlParse`] if the XML is malformed.
pub fn import_sumo_routes(path: &Path) -> Result<SumoRouteImportResult, NetError> {
    let xml_bytes = std::fs::read(path)?;
    let mut reader = Reader::from_reader(xml_bytes.as_slice());
    reader.config_mut().trim_text(true);

    let mut warnings: Vec<String> = Vec::new();

    let mut vtypes: HashMap<String, VTypeEntry> = HashMap::new();
    let mut vehicles: Vec<SumoVehicleDef> = Vec::new();
    let mut persons: Vec<SumoPersonDef> = Vec::new();

    // Parsing state for nested elements.
    let mut current_vehicle: Option<VehicleBuilder> = None;
    let mut current_person: Option<PersonBuilder> = None;
    let mut in_vtype_dist = false;

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                handle_demand_element(
                    e,
                    &mut current_vehicle,
                    &mut current_person,
                    &mut vtypes,
                    &mut vehicles,
                    &mut in_vtype_dist,
                    &mut warnings,
                );
            }
            Ok(Event::Empty(ref e)) => {
                handle_demand_element(
                    e,
                    &mut current_vehicle,
                    &mut current_person,
                    &mut vtypes,
                    &mut vehicles,
                    &mut in_vtype_dist,
                    &mut warnings,
                );
                // Self-closing elements also need finalization.
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                finalize_demand_tag(
                    &tag,
                    &mut current_vehicle,
                    &mut current_person,
                    &mut vehicles,
                    &mut persons,
                    &mut in_vtype_dist,
                );
            }
            Ok(Event::End(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                finalize_demand_tag(
                    &tag,
                    &mut current_vehicle,
                    &mut current_person,
                    &mut vehicles,
                    &mut persons,
                    &mut in_vtype_dist,
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

    Ok((vehicles, persons, warnings))
}

// ---------------------------------------------------------------------------
// Event dispatch and finalization
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn handle_demand_element(
    e: &BytesStart<'_>,
    current_vehicle: &mut Option<VehicleBuilder>,
    current_person: &mut Option<PersonBuilder>,
    vtypes: &mut HashMap<String, VTypeEntry>,
    vehicles: &mut Vec<SumoVehicleDef>,
    in_vtype_dist: &mut bool,
    warnings: &mut Vec<String>,
) {
    let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
    match tag.as_str() {
        "vType" => {
            if let Some((id, entry)) = parse_vtype(e, warnings) {
                vtypes.insert(id, entry);
            }
        }
        "vTypeDistribution" => {
            *in_vtype_dist = true;
        }
        "vehicle" => {
            *current_vehicle = Some(parse_vehicle_start(e, vtypes, warnings));
        }
        "trip" => {
            if let Some(veh) = parse_trip(e, vtypes, warnings) {
                vehicles.push(veh);
            }
        }
        "flow" => {
            vehicles.extend(parse_flow(e, vtypes, warnings));
        }
        "route" => {
            if let Some(vb) = current_vehicle {
                parse_route_element(e, vb, warnings);
            }
        }
        "person" => {
            *current_person = Some(parse_person_start(e, warnings));
        }
        "walk" => {
            if let Some(pb) = current_person {
                parse_walk(e, pb, warnings);
            }
        }
        "ride" => {
            if let Some(pb) = current_person {
                parse_ride(e, pb, warnings);
            }
        }
        "calibrator" => {
            parse_calibrator(e, warnings);
        }
        "routes" | "param" => { /* container */ }
        _ => {}
    }
}

fn finalize_demand_tag(
    tag: &str,
    current_vehicle: &mut Option<VehicleBuilder>,
    current_person: &mut Option<PersonBuilder>,
    vehicles: &mut Vec<SumoVehicleDef>,
    persons: &mut Vec<SumoPersonDef>,
    in_vtype_dist: &mut bool,
) {
    match tag {
        "vehicle" => {
            if let Some(vb) = current_vehicle.take() {
                vehicles.push(vb.build());
            }
        }
        "person" => {
            if let Some(pb) = current_person.take() {
                persons.push(pb.build());
            }
        }
        "vTypeDistribution" => {
            *in_vtype_dist = false;
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

struct VehicleBuilder {
    id: String,
    vtype: String,
    depart: f64,
    route: Vec<String>,
    params: SumoVehicleParams,
}

impl VehicleBuilder {
    fn build(self) -> SumoVehicleDef {
        SumoVehicleDef {
            id: self.id,
            vtype: self.vtype,
            depart: self.depart,
            route: self.route,
            params: self.params,
        }
    }
}

struct PersonBuilder {
    id: String,
    depart: f64,
    stages: Vec<SumoStage>,
}

impl PersonBuilder {
    fn build(self) -> SumoPersonDef {
        SumoPersonDef {
            id: self.id,
            depart: self.depart,
            stages: self.stages,
        }
    }
}

// ---------------------------------------------------------------------------
// Element parsers
// ---------------------------------------------------------------------------

fn parse_vtype(
    e: &BytesStart<'_>,
    warnings: &mut Vec<String>,
) -> Option<(String, VTypeEntry)> {
    let mut id = String::new();
    let mut params = SumoVehicleParams::default();
    let mut probability = None;

    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let val = String::from_utf8_lossy(&attr.value).to_string();
        match key.as_str() {
            "id" => id = val,
            "accel" => params.accel = val.parse().unwrap_or(params.accel),
            "decel" => params.decel = val.parse().unwrap_or(params.decel),
            "maxSpeed" => params.max_speed = val.parse().unwrap_or(params.max_speed),
            "sigma" => params.sigma = val.parse().ok(),
            "minGap" => params.min_gap = val.parse().unwrap_or(params.min_gap),
            "length" => params.length = val.parse().unwrap_or(params.length),
            "carFollowModel" => {
                params.car_follow_model = match val.as_str() {
                    "Krauss" | "krauss" => CarFollowModelType::Krauss,
                    "IDM" | "idm" => CarFollowModelType::IDM,
                    other => {
                        warnings.push(format!(
                            "Unmapped carFollowModel \"{}\" on vType \"{}\", using Other",
                            other, id
                        ));
                        CarFollowModelType::Other(other.to_string())
                    }
                };
            }
            "probability" => probability = val.parse().ok(),
            _ => {
                warnings.push(format!(
                    "Unmapped attribute on <vType id=\"{}\">: {}=\"{}\"",
                    id, key, val
                ));
            }
        }
    }

    if id.is_empty() {
        return None;
    }

    Some((id, VTypeEntry { params, probability }))
}

fn resolve_params(vtype: &str, vtypes: &HashMap<String, VTypeEntry>) -> SumoVehicleParams {
    vtypes
        .get(vtype)
        .map(|e| e.params.clone())
        .unwrap_or_default()
}

fn parse_vehicle_start(
    e: &BytesStart<'_>,
    vtypes: &HashMap<String, VTypeEntry>,
    warnings: &mut Vec<String>,
) -> VehicleBuilder {
    let mut id = String::new();
    let mut vtype = String::new();
    let mut depart = 0.0;

    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let val = String::from_utf8_lossy(&attr.value).to_string();
        match key.as_str() {
            "id" => id = val,
            "type" => vtype = val,
            "depart" => depart = val.parse().unwrap_or(0.0),
            _ => {
                warnings.push(format!(
                    "Unmapped attribute on <vehicle id=\"{}\">: {}=\"{}\"",
                    id, key, val
                ));
            }
        }
    }

    let params = resolve_params(&vtype, vtypes);

    VehicleBuilder {
        id,
        vtype,
        depart,
        route: Vec::new(),
        params,
    }
}

fn parse_route_element(
    e: &BytesStart<'_>,
    vb: &mut VehicleBuilder,
    warnings: &mut Vec<String>,
) {
    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let val = String::from_utf8_lossy(&attr.value).to_string();
        match key.as_str() {
            "edges" => {
                vb.route = val.split_whitespace().map(String::from).collect();
            }
            _ => {
                warnings.push(format!(
                    "Unmapped attribute on <route> of vehicle \"{}\": {}=\"{}\"",
                    vb.id, key, val
                ));
            }
        }
    }
}

fn parse_trip(
    e: &BytesStart<'_>,
    vtypes: &HashMap<String, VTypeEntry>,
    warnings: &mut Vec<String>,
) -> Option<SumoVehicleDef> {
    let mut id = String::new();
    let mut vtype = String::new();
    let mut depart = 0.0;
    let mut from = String::new();
    let mut to = String::new();

    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let val = String::from_utf8_lossy(&attr.value).to_string();
        match key.as_str() {
            "id" => id = val,
            "type" => vtype = val,
            "depart" => depart = val.parse().unwrap_or(0.0),
            "from" => from = val,
            "to" => to = val,
            _ => {
                warnings.push(format!(
                    "Unmapped attribute on <trip id=\"{}\">: {}=\"{}\"",
                    id, key, val
                ));
            }
        }
    }

    let params = resolve_params(&vtype, vtypes);

    Some(SumoVehicleDef {
        id,
        vtype,
        depart,
        route: vec![from, to],
        params,
    })
}

fn parse_flow(
    e: &BytesStart<'_>,
    vtypes: &HashMap<String, VTypeEntry>,
    warnings: &mut Vec<String>,
) -> Vec<SumoVehicleDef> {
    let mut id = String::new();
    let mut vtype = String::new();
    let mut begin = 0.0_f64;
    let mut end = 3600.0_f64;
    let mut number = 0_u32;
    let mut from = String::new();
    let mut to = String::new();

    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let val = String::from_utf8_lossy(&attr.value).to_string();
        match key.as_str() {
            "id" => id = val,
            "type" => vtype = val,
            "begin" => begin = val.parse().unwrap_or(0.0),
            "end" => end = val.parse().unwrap_or(3600.0),
            "number" => number = val.parse().unwrap_or(0),
            "from" => from = val,
            "to" => to = val,
            _ => {
                warnings.push(format!(
                    "Unmapped attribute on <flow id=\"{}\">: {}=\"{}\"",
                    id, key, val
                ));
            }
        }
    }

    if number == 0 {
        return Vec::new();
    }

    let params = resolve_params(&vtype, vtypes);
    let interval = (end - begin) / f64::from(number);

    (0..number)
        .map(|i| {
            let depart = begin + f64::from(i) * interval;
            SumoVehicleDef {
                id: format!("{}_{}", id, i),
                vtype: vtype.clone(),
                depart,
                route: vec![from.clone(), to.clone()],
                params: params.clone(),
            }
        })
        .collect()
}

fn parse_person_start(
    e: &BytesStart<'_>,
    warnings: &mut Vec<String>,
) -> PersonBuilder {
    let mut id = String::new();
    let mut depart = 0.0;

    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let val = String::from_utf8_lossy(&attr.value).to_string();
        match key.as_str() {
            "id" => id = val,
            "depart" => depart = val.parse().unwrap_or(0.0),
            _ => {
                warnings.push(format!(
                    "Unmapped attribute on <person id=\"{}\">: {}=\"{}\"",
                    id, key, val
                ));
            }
        }
    }

    PersonBuilder {
        id,
        depart,
        stages: Vec::new(),
    }
}

fn parse_walk(
    e: &BytesStart<'_>,
    pb: &mut PersonBuilder,
    warnings: &mut Vec<String>,
) {
    let mut from = String::new();
    let mut to = String::new();

    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let val = String::from_utf8_lossy(&attr.value).to_string();
        match key.as_str() {
            "from" => from = val,
            "to" => to = val,
            _ => {
                warnings.push(format!(
                    "Unmapped attribute on <walk> of person \"{}\": {}=\"{}\"",
                    pb.id, key, val
                ));
            }
        }
    }

    pb.stages.push(SumoStage::Walk { from, to });
}

fn parse_ride(
    e: &BytesStart<'_>,
    pb: &mut PersonBuilder,
    warnings: &mut Vec<String>,
) {
    let mut from = String::new();
    let mut to = String::new();
    let mut lines = String::new();

    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let val = String::from_utf8_lossy(&attr.value).to_string();
        match key.as_str() {
            "from" => from = val,
            "to" => to = val,
            "lines" => lines = val,
            _ => {
                warnings.push(format!(
                    "Unmapped attribute on <ride> of person \"{}\": {}=\"{}\"",
                    pb.id, key, val
                ));
            }
        }
    }

    pb.stages.push(SumoStage::Ride { from, to, lines });
}

fn parse_calibrator(e: &BytesStart<'_>, warnings: &mut Vec<String>) {
    let mut id = String::new();

    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let val = String::from_utf8_lossy(&attr.value).to_string();
        match key.as_str() {
            "id" => id = val,
            "edge" | "pos" => { /* known */ }
            _ => {
                warnings.push(format!(
                    "Unmapped attribute on <calibrator id=\"{}\">: {}=\"{}\"",
                    id, key, val
                ));
            }
        }
    }

    warnings.push(format!(
        "calibrator \"{}\" parsed (best-effort, not fully supported)",
        id
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_params_are_krauss() {
        let p = SumoVehicleParams::default();
        assert!(matches!(p.car_follow_model, CarFollowModelType::Krauss));
    }

    #[test]
    fn resolve_unknown_vtype_returns_defaults() {
        let vtypes = HashMap::new();
        let p = resolve_params("nonexistent", &vtypes);
        assert!((p.accel - 2.6).abs() < 0.01);
    }
}
