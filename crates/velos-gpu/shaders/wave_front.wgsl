// Wave-front dispatch compute shader for car-following physics.
//
// Dispatch pattern: one workgroup per lane, only thread 0 active.
// Processes agents front-to-back (leader first) within each lane,
// so followers read the leader's ALREADY-UPDATED position/speed
// from the current timestep (Gauss-Seidel ordering).
//
// Branches on cf_model tag: 0 = IDM, 1 = Krauss.
// Uses fixed-point Q16.16 for position and Q12.20 for speed storage.
// Uses f32 intermediates for physics calculations.

// ============================================================
// Fixed-point constants and helpers (inlined from fixed_point.wgsl)
// ============================================================

const POS_SCALE: f32 = 65536.0;
const SPD_SCALE: f32 = 1048576.0;

fn f32_to_fixpos(v: f32) -> i32 {
    return i32(round(v * POS_SCALE));
}

fn fixpos_to_f32(v: i32) -> f32 {
    return f32(v) / POS_SCALE;
}

fn f32_to_fixspd(v: f32) -> i32 {
    return i32(round(v * SPD_SCALE));
}

fn fixspd_to_f32(v: i32) -> f32 {
    return f32(v) / SPD_SCALE;
}

/// Multiply speed (Q12.20) by dt (as f32 seconds) -> displacement in f32 metres.
/// We convert speed to f32, multiply, then convert back to Q16.16 for position update.
fn speed_times_dt_f32(speed_fix: i32, dt: f32) -> f32 {
    return fixspd_to_f32(speed_fix) * dt;
}

// ============================================================
// PCG hash-based RNG (deterministic per agent per step)
// ============================================================

fn pcg_hash(input: u32) -> u32 {
    var state = input * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

/// Deterministic random float [0, 1) for a given agent and step.
fn rand_float(rng_state: u32, step: u32) -> f32 {
    let hash = pcg_hash(rng_state ^ (step * 1664525u + 1013904223u));
    return f32(hash) / 4294967296.0;
}

// ============================================================
// Car-following model constants
// ============================================================

const CF_IDM: u32 = 0u;
const CF_KRAUSS: u32 = 1u;

// Physical limits (not tunable per vehicle type)
const IDM_MAX_DECEL: f32 = -9.0; // hard deceleration limit (physical)
const KRAUSS_TAU: f32 = 1.0;     // reaction time (shared across types)

// ============================================================
// Buffer layout
// ============================================================

struct Params {
    agent_count: u32,
    dt: f32,
    step_counter: u32,
    emergency_count: u32,
    sign_count: u32,
    sim_time: f32,
    _pad0: u32,
    _pad1: u32,
}

struct AgentState {
    edge_id: u32,
    lane_idx: u32,
    position: i32,      // Q16.16
    lateral: i32,       // Q8.8 in i32
    speed: i32,         // Q12.20
    acceleration: i32,  // Q12.20
    cf_model: u32,      // 0=IDM, 1=Krauss
    rng_state: u32,
    vehicle_type: u32,  // 0=Motorbike..6=Pedestrian
    flags: u32,         // bit0=bus_dwelling, bit1=emergency_active, bit2=yielding
}

// Vehicle type constants (matches Rust VehicleType enum order)
const VT_MOTORBIKE: u32 = 0u;
const VT_CAR: u32 = 1u;
const VT_BUS: u32 = 2u;
const VT_BICYCLE: u32 = 3u;
const VT_TRUCK: u32 = 4u;
const VT_EMERGENCY: u32 = 5u;
const VT_PEDESTRIAN: u32 = 6u;

// Flag bitfield constants
const FLAG_BUS_DWELLING: u32 = 1u;
const FLAG_EMERGENCY_ACTIVE: u32 = 2u;
const FLAG_YIELDING: u32 = 4u;

// Emergency vehicle data for yield cone detection (max 16 active)
struct EmergencyVehicle {
    pos_x: f32,
    pos_y: f32,
    heading: f32,
    _pad: f32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> agents: array<AgentState>;
@group(0) @binding(2) var<storage, read> lane_offsets: array<u32>;
@group(0) @binding(3) var<storage, read> lane_counts: array<u32>;
@group(0) @binding(4) var<storage, read> lane_agents: array<u32>;
@group(0) @binding(5) var<storage, read> emergency_vehicles: array<EmergencyVehicle>;

// Traffic sign data (matches Rust GpuSign, 16 bytes per sign)
struct GpuSign {
    sign_type: u32,   // 0=SpeedLimit, 1=Stop, 2=Yield, 3=NoTurn, 4=SchoolZone
    value: f32,       // speed limit (m/s), gap time (s), etc.
    edge_id: u32,     // edge where sign is located
    offset_m: f32,    // position along the edge (metres)
}

@group(0) @binding(6) var<storage, read> signs: array<GpuSign>;

// Per-vehicle-type parameters (matches Rust GpuVehicleParams layout)
// Indexed by vehicle_type: 0=Motorbike..6=Pedestrian
struct VehicleTypeParams {
    v0: f32,            // desired free-flow speed (m/s)
    s0: f32,            // minimum gap at standstill (m)
    t_headway: f32,     // desired time headway (s)
    a: f32,             // IDM max acceleration (m/s^2)
    b: f32,             // IDM comfortable deceleration (m/s^2)
    krauss_accel: f32,  // Krauss max acceleration (m/s^2)
    krauss_decel: f32,  // Krauss max deceleration (m/s^2)
    krauss_sigma: f32,  // Krauss driver imperfection [0, 1]
}

@group(0) @binding(7) var<uniform> vehicle_params: array<VehicleTypeParams, 7>;

// Per-agent perception result from GPU gather pass (32 bytes, matches Rust PerceptionResult).
// Written by perception.wgsl, read here for HCMC behavior functions.
struct PerceptionResult {
    leader_speed: f32,       // m/s, 0.0 if no leader
    leader_gap: f32,         // m, 9999.0 if no leader
    signal_state: u32,       // 0=green, 1=amber, 2=red, 3=none
    signal_distance: f32,    // m to next signal
    congestion_own_route: f32,
    congestion_area: f32,
    sign_speed_limit: f32,   // m/s, 0.0 if none
    perc_flags: u32,         // bit0=route_blocked, bit1=emergency_nearby
}

@group(0) @binding(8) var<storage, read> perception_results: array<PerceptionResult>;

// Signal state constants for perception
const SIGNAL_GREEN: u32 = 0u;
const SIGNAL_AMBER: u32 = 1u;
const SIGNAL_RED: u32 = 2u;
const SIGNAL_NONE: u32 = 3u;

// Sign type constants
const SIGN_SPEED_LIMIT: u32 = 0u;
const SIGN_STOP: u32 = 1u;
const SIGN_YIELD: u32 = 2u;
const SIGN_NO_TURN: u32 = 3u;
const SIGN_SCHOOL_ZONE: u32 = 4u;

// Sign interaction constants
const SIGN_EFFECT_RANGE: f32 = 50.0;     // speed limit effect range (metres)
const SIGN_STOP_RANGE: f32 = 2.0;        // stop sign trigger range (metres)
const SCHOOL_ZONE_SPEED: f32 = 5.56;     // 20 km/h in m/s

// ============================================================
// IDM acceleration (matches CPU idm.rs)
// ============================================================

fn safe_pow4(x: f32) -> f32 {
    let x2 = x * x;
    return x2 * x2;
}

fn idm_acceleration(v: f32, gap: f32, delta_v: f32, vt: u32) -> f32 {
    let vp = vehicle_params[vt];

    // Free-road term: 1 - (v/v0)^4
    let v_ratio = v / vp.v0;
    let free_term = 1.0 - safe_pow4(v_ratio);

    // Desired dynamical gap s*
    let v_eff = max(v, 0.1); // kickstart
    let ab_sqrt = sqrt(vp.a * vp.b);
    let s_star = vp.s0
        + v_eff * vp.t_headway
        + (v * delta_v) / (2.0 * ab_sqrt);

    // Interaction term
    let gap_eff = max(gap, 0.01); // floor to avoid div-by-zero
    let gap_ratio = s_star / gap_eff;
    let interaction = gap_ratio * gap_ratio;

    // IDM acceleration, clamped
    let accel = vp.a * (free_term - interaction);
    return clamp(accel, IDM_MAX_DECEL, vp.a);
}

// ============================================================
// Krauss car-following (matches CPU krauss.rs)
// ============================================================

fn krauss_safe_speed(gap: f32, leader_speed: f32, own_speed: f32, vt: u32) -> f32 {
    let vp = vehicle_params[vt];
    let denominator = (leader_speed + own_speed) / (2.0 * vp.krauss_decel) + KRAUSS_TAU;
    let numerator = gap - leader_speed * KRAUSS_TAU;
    let v_safe = leader_speed + numerator / denominator;
    return max(v_safe, 0.0);
}

fn krauss_update(own_speed: f32, gap: f32, leader_speed: f32, dt: f32, rng_val: f32, vt: u32) -> f32 {
    let vp = vehicle_params[vt];
    let v_safe = krauss_safe_speed(gap, leader_speed, own_speed, vt);
    let v_desired = min(own_speed + vp.krauss_accel * dt, vp.v0);
    let v_next = min(v_desired, v_safe);

    // Dawdle
    let dawdle_base = select(vp.krauss_accel, v_next, v_next < vp.krauss_accel);
    let v_dawdled = v_next - vp.krauss_sigma * min(v_next, dawdle_base) * rng_val;
    return max(v_dawdled, 0.0);
}

// ============================================================
// Emergency vehicle constants and helpers
// ============================================================

const EMERGENCY_YIELD_RANGE: f32 = 50.0;    // detection range (metres)
const EMERGENCY_CONE_COS: f32 = 0.7071;     // cos(45 degrees) = half-angle of 90-degree cone
const EMERGENCY_YIELD_SPEED: f32 = 1.4;     // yielding agents slow to 1.4 m/s (5 km/h)
const EMERGENCY_INTERSECTION_SPEED: f32 = 5.0; // emergency vehicles decelerate to 5 m/s at intersections

/// Check if an agent should yield to any active emergency vehicle.
/// Sets FLAG_YIELDING on the agent if within any emergency vehicle's yield cone.
/// Early-exits when emergency_count == 0 (zero cost in normal operation).
fn check_emergency_yield(agent: ptr<function, AgentState>) {
    if params.emergency_count == 0u {
        return;
    }

    // Skip emergency vehicles themselves -- they don't yield to each other
    if (*agent).vehicle_type == VT_EMERGENCY {
        return;
    }

    let agent_pos_x = fixpos_to_f32((*agent).position);
    let agent_pos_y = f32((*agent).lateral) / 256.0; // Q8.8 lateral -> f32

    let em_count = min(params.emergency_count, 16u);
    for (var e = 0u; e < em_count; e = e + 1u) {
        let ev = emergency_vehicles[e];

        // Vector from emergency to agent
        let dx = agent_pos_x - ev.pos_x;
        let dy = agent_pos_y - ev.pos_y;
        let dist_sq = dx * dx + dy * dy;

        // Range check
        if dist_sq > EMERGENCY_YIELD_RANGE * EMERGENCY_YIELD_RANGE {
            continue;
        }

        let dist = sqrt(dist_sq);
        if dist < 0.001 {
            continue;
        }

        // Cone direction from emergency heading
        let dir_x = cos(ev.heading);
        let dir_y = sin(ev.heading);

        // Angle check: dot product of normalized vectors
        let dot = (dx * dir_x + dy * dir_y) / dist;
        if dot >= EMERGENCY_CONE_COS {
            // Agent is in cone -- set yielding flag
            (*agent).flags = (*agent).flags | FLAG_YIELDING;
            return;
        }
    }
}

// ============================================================
// Traffic sign interaction
// ============================================================

/// Apply traffic sign effects to an agent's desired speed.
/// Scans the sign buffer for signs on the agent's current edge.
/// Returns the clamped desired speed after sign effects.
fn handle_sign_interaction(agent: ptr<function, AgentState>, desired_speed: f32) -> f32 {
    if params.sign_count == 0u {
        return desired_speed;
    }

    var speed = desired_speed;
    let agent_pos = fixpos_to_f32((*agent).position);
    let agent_edge = (*agent).edge_id;
    let s_count = min(params.sign_count, arrayLength(&signs));

    for (var s = 0u; s < s_count; s = s + 1u) {
        let sign = signs[s];

        // Only process signs on the agent's current edge
        if sign.edge_id != agent_edge {
            continue;
        }

        let distance = abs(agent_pos - sign.offset_m);

        switch sign.sign_type {
            case SIGN_SPEED_LIMIT: {
                // Within 50m: clamp desired speed to posted limit
                if distance <= SIGN_EFFECT_RANGE {
                    speed = min(speed, sign.value);
                }
            }
            case SIGN_STOP: {
                // Within 2m and still moving: set speed to 0
                // CPU tracks gap acceptance timer for restart
                if distance <= SIGN_STOP_RANGE {
                    speed = 0.0;
                }
            }
            case SIGN_SCHOOL_ZONE: {
                // Treated as speed limit with reduced value when active
                // sim_time check: sign.value encodes the limit (5.56 m/s)
                // Time-window enforcement is done on CPU; GPU always applies
                // the reduced speed when the sign is present in the buffer
                if distance <= SIGN_EFFECT_RANGE {
                    speed = min(speed, sign.value);
                }
            }
            // SIGN_YIELD: handled on CPU (needs conflicting traffic info)
            // SIGN_NO_TURN: handled at pathfinding level (Phase 7)
            default: {
                // No GPU-side action for Yield and NoTurn
            }
        }
    }

    return speed;
}

// ============================================================
// HCMC behavior: red-light creep (matches CPU sublane.rs)
// ============================================================

// Only motorbikes and bicycles creep forward at red lights.
// Creep speed ramps linearly with distance to stop line, capped at 0.3 m/s.
const CREEP_MAX_SPEED: f32 = 0.3;
const CREEP_DISTANCE_SCALE: f32 = 5.0;
const CREEP_MIN_DISTANCE: f32 = 0.5;

fn red_light_creep_speed(distance_to_stop: f32, vehicle_type: u32) -> f32 {
    // Only motorbikes and bicycles creep
    if vehicle_type != VT_MOTORBIKE && vehicle_type != VT_BICYCLE {
        return 0.0;
    }
    // Too close to stop line — already at front of swarm
    if distance_to_stop < CREEP_MIN_DISTANCE {
        return 0.0;
    }
    let ramp = min(distance_to_stop / CREEP_DISTANCE_SCALE, 1.0);
    return CREEP_MAX_SPEED * ramp;
}

// ============================================================
// HCMC behavior: intersection gap acceptance (matches CPU intersection.rs)
// ============================================================

// Size intimidation: larger approaching vehicles increase required TTC gap.
const GAP_MAX_WAIT_TIME: f32 = 5.0;
const GAP_FORCED_ACCEPTANCE_FACTOR: f32 = 0.5;
const GAP_WAIT_REDUCTION_RATE: f32 = 0.1;

fn size_factor(approaching_type: u32) -> f32 {
    switch approaching_type {
        case 4u, 2u: { return 1.3; }   // Truck, Bus
        case 5u: { return 2.0; }        // Emergency
        case 0u, 3u: { return 0.8; }    // Motorbike, Bicycle
        case 6u: { return 0.5; }        // Pedestrian
        default: { return 1.0; }        // Car
    }
}

/// Determine if a vehicle should proceed through an unsignalized intersection.
/// Returns true if the gap is acceptable (TTC exceeds effective threshold).
fn intersection_gap_acceptance(
    other_type: u32, ttc: f32, ttc_threshold: f32, wait_time: f32,
) -> bool {
    let sf = size_factor(other_type);
    var wait_mod: f32;
    if wait_time >= GAP_MAX_WAIT_TIME {
        wait_mod = GAP_FORCED_ACCEPTANCE_FACTOR;
    } else {
        wait_mod = 1.0 - GAP_WAIT_REDUCTION_RATE * min(wait_time, GAP_MAX_WAIT_TIME);
    }
    let effective = ttc_threshold * sf * wait_mod;
    return ttc > effective;
}

// ============================================================
// Wave-front kernel: one workgroup per lane, thread 0 only
// ============================================================

@compute @workgroup_size(64)
fn wave_front_update(
    @builtin(workgroup_id) wg_id: vec3<u32>,
    @builtin(local_invocation_id) local_id: vec3<u32>,
) {
    // Only thread 0 in each workgroup is active
    if local_id.x != 0u {
        return;
    }

    // 2D dispatch to support > 65535 lanes: lane = x + y * 65535
    let lane_idx = wg_id.x + wg_id.y * 65535u;
    if lane_idx >= arrayLength(&lane_counts) {
        return;
    }
    let count = lane_counts[lane_idx];
    if count == 0u {
        return;
    }

    let offset = lane_offsets[lane_idx];
    let dt = params.dt;
    let step = params.step_counter;

    // Process agents front-to-back (index 0 = leader, already sorted by position descending).
    // Leader has no leader ahead -- uses free-flow.
    for (var i = 0u; i < count; i = i + 1u) {
        let agent_idx = lane_agents[offset + i];
        var agent = agents[agent_idx];
        let own_speed_f32 = fixspd_to_f32(agent.speed);

        var gap: f32;
        var delta_v: f32;
        var leader_speed_f32: f32;

        if i == 0u {
            // Leader: no vehicle ahead -- large gap, free flow
            gap = 1000.0;
            delta_v = 0.0;
            leader_speed_f32 = own_speed_f32;
        } else {
            // Read leader's ALREADY-UPDATED state (wave-front guarantee)
            let leader_idx = lane_agents[offset + i - 1u];
            let leader = agents[leader_idx];
            leader_speed_f32 = fixspd_to_f32(leader.speed);
            let leader_pos_f32 = fixpos_to_f32(leader.position);
            let own_pos_f32 = fixpos_to_f32(agent.position);
            gap = max(leader_pos_f32 - own_pos_f32, 0.0);
            delta_v = own_speed_f32 - leader_speed_f32;
        }

        // Pre-processing: emergency vehicle intersection deceleration
        // Emergency vehicles with active sirens decelerate to 5 m/s safety speed
        // at intersections (when they are the lane leader with no vehicle ahead).
        let is_active_emergency = agent.vehicle_type == VT_EMERGENCY
            && (agent.flags & FLAG_EMERGENCY_ACTIVE) != 0u;

        // Branch on car-following model
        var new_speed_f32: f32;
        var accel_f32: f32;

        let vt = agent.vehicle_type;

        if agent.cf_model == CF_IDM {
            accel_f32 = idm_acceleration(own_speed_f32, gap, delta_v, vt);
            new_speed_f32 = own_speed_f32 + accel_f32 * dt;
            // Stopping guard: if would go negative, stop
            if new_speed_f32 < 0.0 {
                new_speed_f32 = 0.0;
                accel_f32 = -own_speed_f32 / max(dt, 0.001);
            }
        } else {
            // CF_KRAUSS
            let rng_val = rand_float(agent.rng_state, step);
            new_speed_f32 = krauss_update(own_speed_f32, gap, leader_speed_f32, dt, rng_val, vt);
            accel_f32 = (new_speed_f32 - own_speed_f32) / max(dt, 0.001);
        }

        // Clamp speed >= 0
        new_speed_f32 = max(new_speed_f32, 0.0);

        // Post-processing: emergency vehicle intersection speed limit
        if is_active_emergency && i == 0u {
            // Lane leader emergency vehicle: cap speed at intersection safety limit
            new_speed_f32 = min(new_speed_f32, EMERGENCY_INTERSECTION_SPEED);
        }

        // Post-processing: check if this agent should yield to an emergency vehicle
        check_emergency_yield(&agent);
        if (agent.flags & FLAG_YIELDING) != 0u {
            // Override speed to yield target
            new_speed_f32 = min(new_speed_f32, EMERGENCY_YIELD_SPEED);
        }

        // Post-processing: traffic sign interaction (speed limits, stop signs, school zones)
        new_speed_f32 = handle_sign_interaction(&agent, new_speed_f32);

        // Post-processing: HCMC perception-driven behaviors
        // Read perception data for this agent (bounds-checked).
        if agent_idx < arrayLength(&perception_results) {
            let perc = perception_results[agent_idx];

            // Red-light creep: motorbikes/bicycles inch forward at red signals
            if perc.signal_state == SIGNAL_RED {
                let creep = red_light_creep_speed(perc.signal_distance, agent.vehicle_type);
                if creep > 0.0 {
                    // Override: use creep speed instead of full stop
                    new_speed_f32 = max(new_speed_f32, creep);
                }
            }

            // Unsignalized intersection gap acceptance:
            // When no signal present (signal_state == 3/none) and a leader is nearby,
            // use gap acceptance to decide whether to proceed or decelerate.
            if perc.signal_state == SIGNAL_NONE && perc.leader_gap < 100.0 {
                // Estimate TTC from leader gap and closing speed
                let closing_speed = max(own_speed_f32 - perc.leader_speed, 0.01);
                let ttc = perc.leader_gap / closing_speed;

                // Base TTC threshold from vehicle type headway (seconds)
                let base_threshold = vehicle_params[vt].t_headway;

                // Approximate wait_time: if nearly stopped, agent is waiting
                // Use flags bit3 as wait accumulator indicator (0 = not waiting)
                var wait_time = 0.0;
                if own_speed_f32 < 0.5 {
                    // Estimate wait from sim_time modulo — crude but GPU-friendly
                    // In practice wait tracking happens on CPU; GPU uses 0.0 as safe default
                    wait_time = 0.0;
                }

                // Leader vehicle type unknown from perception — use VT_CAR as
                // neutral default (size_factor=1.0). Full type-aware gap acceptance
                // runs on CPU with complete neighbor data.
                let accept = intersection_gap_acceptance(
                    VT_CAR,
                    ttc,
                    base_threshold,
                    wait_time,
                );
                if !accept {
                    // Gap not safe — decelerate to stop
                    new_speed_f32 = max(new_speed_f32 - vehicle_params[vt].b * dt, 0.0);
                }
            }
        }

        // Update position: pos += avg_speed * dt (trapezoidal for smoother integration)
        let avg_speed = (own_speed_f32 + new_speed_f32) * 0.5;
        let displacement = avg_speed * dt;
        let displacement_fix = f32_to_fixpos(displacement);

        // Write updated state in-place (wave-front: followers see these updates)
        agent.position = agent.position + displacement_fix;
        agent.speed = f32_to_fixspd(new_speed_f32);
        agent.acceleration = f32_to_fixspd(accel_f32);
        agents[agent_idx] = agent;
    }
}
