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

// IDM default parameters
const IDM_V0: f32 = 13.89;       // desired speed 50 km/h
const IDM_S0: f32 = 2.0;         // min gap at standstill
const IDM_T_HEADWAY: f32 = 1.5;  // desired time headway
const IDM_A: f32 = 1.5;          // max acceleration
const IDM_B: f32 = 3.0;          // comfortable deceleration
const IDM_MAX_DECEL: f32 = -9.0; // hard deceleration limit

// Krauss default parameters
const KRAUSS_ACCEL: f32 = 2.6;
const KRAUSS_DECEL: f32 = 4.5;
const KRAUSS_SIGMA: f32 = 0.5;
const KRAUSS_TAU: f32 = 1.0;
const KRAUSS_MAX_SPEED: f32 = 13.89;

// ============================================================
// Buffer layout
// ============================================================

struct Params {
    agent_count: u32,
    dt: f32,
    step_counter: u32,
    _pad: u32,
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
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> agents: array<AgentState>;
@group(0) @binding(2) var<storage, read> lane_offsets: array<u32>;
@group(0) @binding(3) var<storage, read> lane_counts: array<u32>;
@group(0) @binding(4) var<storage, read> lane_agents: array<u32>;

// ============================================================
// IDM acceleration (matches CPU idm.rs)
// ============================================================

fn safe_pow4(x: f32) -> f32 {
    let x2 = x * x;
    return x2 * x2;
}

fn idm_acceleration(v: f32, gap: f32, delta_v: f32) -> f32 {
    // Free-road term: 1 - (v/v0)^4
    let v_ratio = v / IDM_V0;
    let free_term = 1.0 - safe_pow4(v_ratio);

    // Desired dynamical gap s*
    let v_eff = max(v, 0.1); // kickstart
    let ab_sqrt = sqrt(IDM_A * IDM_B);
    let s_star = IDM_S0
        + v_eff * IDM_T_HEADWAY
        + (v * delta_v) / (2.0 * ab_sqrt);

    // Interaction term
    let gap_eff = max(gap, 0.01); // floor to avoid div-by-zero
    let gap_ratio = s_star / gap_eff;
    let interaction = gap_ratio * gap_ratio;

    // IDM acceleration, clamped
    let accel = IDM_A * (free_term - interaction);
    return clamp(accel, IDM_MAX_DECEL, IDM_A);
}

// ============================================================
// Krauss car-following (matches CPU krauss.rs)
// ============================================================

fn krauss_safe_speed(gap: f32, leader_speed: f32, own_speed: f32) -> f32 {
    let denominator = (leader_speed + own_speed) / (2.0 * KRAUSS_DECEL) + KRAUSS_TAU;
    let numerator = gap - leader_speed * KRAUSS_TAU;
    let v_safe = leader_speed + numerator / denominator;
    return max(v_safe, 0.0);
}

fn krauss_update(own_speed: f32, gap: f32, leader_speed: f32, dt: f32, rng_val: f32) -> f32 {
    let v_safe = krauss_safe_speed(gap, leader_speed, own_speed);
    let v_desired = min(own_speed + KRAUSS_ACCEL * dt, KRAUSS_MAX_SPEED);
    let v_next = min(v_desired, v_safe);

    // Dawdle
    let dawdle_base = select(KRAUSS_ACCEL, v_next, v_next < KRAUSS_ACCEL);
    let v_dawdled = v_next - KRAUSS_SIGMA * min(v_next, dawdle_base) * rng_val;
    return max(v_dawdled, 0.0);
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

        // Branch on car-following model
        var new_speed_f32: f32;
        var accel_f32: f32;

        if agent.cf_model == CF_IDM {
            accel_f32 = idm_acceleration(own_speed_f32, gap, delta_v);
            new_speed_f32 = own_speed_f32 + accel_f32 * dt;
            // Stopping guard: if would go negative, stop
            if new_speed_f32 < 0.0 {
                new_speed_f32 = 0.0;
                accel_f32 = -own_speed_f32 / max(dt, 0.001);
            }
        } else {
            // CF_KRAUSS
            let rng_val = rand_float(agent.rng_state, step);
            new_speed_f32 = krauss_update(own_speed_f32, gap, leader_speed_f32, dt, rng_val);
            accel_f32 = (new_speed_f32 - own_speed_f32) / max(dt, 0.001);
        }

        // Clamp speed >= 0
        new_speed_f32 = max(new_speed_f32, 0.0);

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
