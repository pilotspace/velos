// pedestrian_adaptive.wgsl -- Adaptive pedestrian GPU dispatch with prefix-sum compaction.
//
// 4-pass pipeline:
//   Pass 1 (count_per_cell):  Hash pedestrians to spatial cells, count per cell via atomics.
//   Pass 2 (prefix_sum):      Hillis-Steele exclusive prefix sum on cell_counts -> cell_offsets.
//                              Multi-workgroup: reduce-then-scan (2 sub-dispatches merged into 1 entry point).
//   Pass 3 (scatter):         Scatter pedestrian indices into compacted array using cell_offsets.
//   Pass 4 (social_force_adaptive): Social force computation on non-empty cells only.
//
// Buffer bindings documented at each @group/@binding.

// ============================================================
// Shared types and constants
// ============================================================

struct PedestrianParams {
    ped_count: u32,       // number of pedestrian agents
    cell_count: u32,      // total grid cells (grid_w * grid_h)
    grid_w: u32,          // grid width in cells
    grid_h: u32,          // grid height in cells
    cell_size: f32,       // spatial hash cell size (2.0, 5.0, or 10.0m)
    dt: f32,              // timestep (s)
    // Social force parameters
    a_social: f32,        // repulsion strength (N)
    b_social: f32,        // repulsion range (m)
    tau: f32,             // relaxation time (s)
    desired_speed: f32,   // desired walking speed (m/s)
    lambda: f32,          // anisotropy parameter (0..1)
    max_force: f32,       // max per-neighbor force (N)
    max_speed: f32,       // max pedestrian speed (m/s)
    radius: f32,          // pedestrian body radius (m)
    workgroup_count: u32, // number of workgroups for prefix sum (for multi-WG scan)
    _pad: u32,
}

struct Pedestrian {
    pos_x: f32,
    pos_y: f32,
    vel_x: f32,
    vel_y: f32,
    dest_x: f32,
    dest_y: f32,
    radius: f32,
    _pad: f32,
}

// ============================================================
// Pass 1: Count pedestrians per spatial hash cell
// ============================================================

@group(0) @binding(0) var<uniform> params: PedestrianParams;
@group(0) @binding(1) var<storage, read_write> pedestrians: array<Pedestrian>;
@group(0) @binding(2) var<storage, read_write> cell_counts: array<atomic<u32>>;
@group(0) @binding(3) var<storage, read_write> cell_offsets: array<u32>;
@group(0) @binding(4) var<storage, read_write> compacted_indices: array<u32>;
@group(0) @binding(5) var<storage, read_write> ped_cell_map: array<u32>;
@group(0) @binding(6) var<storage, read_write> scatter_counters: array<atomic<u32>>;
@group(0) @binding(7) var<storage, read_write> workgroup_sums: array<u32>;

fn cell_id(pos_x: f32, pos_y: f32) -> u32 {
    let cx = u32(max(floor(pos_x / params.cell_size), 0.0));
    let cy = u32(max(floor(pos_y / params.cell_size), 0.0));
    let clamped_x = min(cx, params.grid_w - 1u);
    let clamped_y = min(cy, params.grid_h - 1u);
    return clamped_y * params.grid_w + clamped_x;
}

@compute @workgroup_size(256)
fn count_per_cell(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if idx >= params.ped_count {
        return;
    }

    let ped = pedestrians[idx];
    let cid = cell_id(ped.pos_x, ped.pos_y);
    ped_cell_map[idx] = cid;
    atomicAdd(&cell_counts[cid], 1u);
}

// ============================================================
// Pass 2: Multi-workgroup exclusive prefix sum (Hillis-Steele)
//
// Sub-pass A: per-workgroup prefix sum + store workgroup totals
// Sub-pass B: scan workgroup totals (single workgroup)
// Sub-pass C: propagate scanned totals back
//
// These are 3 separate entry points dispatched sequentially.
// ============================================================

const PREFIX_WG_SIZE: u32 = 256u;

var<workgroup> shared_data: array<u32, 256>;

// Sub-pass A: per-workgroup prefix sum, store per-workgroup total
@compute @workgroup_size(256)
fn prefix_sum_local(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
    @builtin(workgroup_id) wg_id: vec3<u32>,
) {
    let global_idx = gid.x;
    let local_idx = lid.x;

    // Load cell count (non-atomic read via bitcast)
    if global_idx < params.cell_count {
        shared_data[local_idx] = atomicLoad(&cell_counts[global_idx]);
    } else {
        shared_data[local_idx] = 0u;
    }
    workgroupBarrier();

    // Hillis-Steele inclusive scan
    var offset = 1u;
    for (var d = 0u; d < 8u; d = d + 1u) {  // log2(256) = 8
        if offset > PREFIX_WG_SIZE { break; }
        var val = 0u;
        if local_idx >= offset {
            val = shared_data[local_idx - offset];
        }
        workgroupBarrier();
        shared_data[local_idx] = shared_data[local_idx] + val;
        workgroupBarrier();
        offset = offset * 2u;
    }

    // Convert inclusive to exclusive: shift right, element 0 = 0
    let inclusive_val = shared_data[local_idx];
    workgroupBarrier();

    if local_idx == 0u {
        shared_data[0u] = 0u;
    } else {
        shared_data[local_idx] = shared_data[local_idx - 1u];
    }
    workgroupBarrier();

    // Write exclusive prefix sum to cell_offsets
    if global_idx < params.cell_count {
        cell_offsets[global_idx] = shared_data[local_idx];
    }

    // Last thread stores the workgroup total (inclusive sum of last element)
    if local_idx == PREFIX_WG_SIZE - 1u {
        workgroup_sums[wg_id.x] = inclusive_val;
    }
}

// Sub-pass B: scan the workgroup sums (single workgroup dispatch)
@compute @workgroup_size(256)
fn prefix_sum_workgroup_sums(
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let local_idx = lid.x;

    if local_idx < params.workgroup_count {
        shared_data[local_idx] = workgroup_sums[local_idx];
    } else {
        shared_data[local_idx] = 0u;
    }
    workgroupBarrier();

    // Hillis-Steele inclusive scan
    var offset = 1u;
    for (var d = 0u; d < 8u; d = d + 1u) {
        if offset > PREFIX_WG_SIZE { break; }
        var val = 0u;
        if local_idx >= offset {
            val = shared_data[local_idx - offset];
        }
        workgroupBarrier();
        shared_data[local_idx] = shared_data[local_idx] + val;
        workgroupBarrier();
        offset = offset * 2u;
    }

    // Convert inclusive to exclusive
    let inclusive_val = shared_data[local_idx];
    workgroupBarrier();

    if local_idx == 0u {
        shared_data[0u] = 0u;
    } else {
        shared_data[local_idx] = shared_data[local_idx - 1u];
    }
    workgroupBarrier();

    if local_idx < params.workgroup_count {
        workgroup_sums[local_idx] = shared_data[local_idx];
    }
}

// Sub-pass C: add scanned workgroup sums back to each element
@compute @workgroup_size(256)
fn prefix_sum_propagate(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(workgroup_id) wg_id: vec3<u32>,
) {
    let global_idx = gid.x;
    if global_idx >= params.cell_count {
        return;
    }

    // Skip workgroup 0 (its offset is 0)
    if wg_id.x > 0u {
        cell_offsets[global_idx] = cell_offsets[global_idx] + workgroup_sums[wg_id.x];
    }
}

// ============================================================
// Pass 3: Scatter pedestrians into compacted index array
// ============================================================

@compute @workgroup_size(256)
fn scatter(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if idx >= params.ped_count {
        return;
    }

    let cid = ped_cell_map[idx];
    let base_offset = cell_offsets[cid];
    let local_offset = atomicAdd(&scatter_counters[cid], 1u);
    compacted_indices[base_offset + local_offset] = idx;
}

// ============================================================
// Pass 4: Social force with adaptive workgroups
//
// One workgroup per non-empty cell. Threads process pedestrians
// within the cell. Neighbor search over 9-cell neighborhood.
// ============================================================

@compute @workgroup_size(64)
fn social_force_adaptive(
    @builtin(workgroup_id) wg_id: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    // wg_id.x = index into non-empty cells (dispatched via indirect or direct with cell_count)
    let cell_idx = wg_id.x;
    if cell_idx >= params.cell_count {
        return;
    }

    // Get pedestrian range for this cell
    let cell_start = cell_offsets[cell_idx];
    var cell_end: u32;
    if cell_idx + 1u < params.cell_count {
        cell_end = cell_offsets[cell_idx + 1u];
    } else {
        cell_end = params.ped_count;
    }

    let cell_ped_count = cell_end - cell_start;
    if cell_ped_count == 0u {
        return;  // empty cell, skip
    }

    // Each thread handles one pedestrian (if within range)
    let local_idx = lid.x;
    if local_idx >= cell_ped_count {
        return;
    }

    let ped_idx = compacted_indices[cell_start + local_idx];
    var ped = pedestrians[ped_idx];

    // Compute cell coordinates for 9-cell neighborhood
    let cx = cell_idx % params.grid_w;
    let cy = cell_idx / params.grid_w;

    // Driving force: (v_desired * e_desired - v_current) / tau
    let dx_dest = ped.dest_x - ped.pos_x;
    let dy_dest = ped.dest_y - ped.pos_y;
    let dist_to_dest = sqrt(dx_dest * dx_dest + dy_dest * dy_dest);

    var driving_x: f32;
    var driving_y: f32;
    if dist_to_dest > 1e-6 {
        let dir_x = dx_dest / dist_to_dest;
        let dir_y = dy_dest / dist_to_dest;
        let desired_vx = params.desired_speed * dir_x;
        let desired_vy = params.desired_speed * dir_y;
        driving_x = (desired_vx - ped.vel_x) / params.tau;
        driving_y = (desired_vy - ped.vel_y) / params.tau;
    } else {
        driving_x = -ped.vel_x / params.tau;
        driving_y = -ped.vel_y / params.tau;
    }

    // Ego direction for anisotropy
    let ego_speed = sqrt(ped.vel_x * ped.vel_x + ped.vel_y * ped.vel_y);
    var ego_dir_x: f32;
    var ego_dir_y: f32;
    if ego_speed > 1e-6 {
        ego_dir_x = ped.vel_x / ego_speed;
        ego_dir_y = ped.vel_y / ego_speed;
    } else if dist_to_dest > 1e-6 {
        ego_dir_x = dx_dest / dist_to_dest;
        ego_dir_y = dy_dest / dist_to_dest;
    } else {
        ego_dir_x = 1.0;
        ego_dir_y = 0.0;
    }

    // Repulsive forces from 9-cell neighborhood
    var repulsion_x = 0.0f;
    var repulsion_y = 0.0f;

    for (var dy_cell: i32 = -1; dy_cell <= 1; dy_cell = dy_cell + 1) {
        for (var dx_cell: i32 = -1; dx_cell <= 1; dx_cell = dx_cell + 1) {
            let nx = i32(cx) + dx_cell;
            let ny = i32(cy) + dy_cell;

            // Bounds check
            if nx < 0 || ny < 0 || u32(nx) >= params.grid_w || u32(ny) >= params.grid_h {
                continue;
            }

            let neighbor_cell = u32(ny) * params.grid_w + u32(nx);
            let n_start = cell_offsets[neighbor_cell];
            var n_end: u32;
            if neighbor_cell + 1u < params.cell_count {
                n_end = cell_offsets[neighbor_cell + 1u];
            } else {
                n_end = params.ped_count;
            }

            for (var j = n_start; j < n_end; j = j + 1u) {
                let other_idx = compacted_indices[j];
                if other_idx == ped_idx {
                    continue;  // skip self
                }

                let other = pedestrians[other_idx];

                // Vector FROM neighbor TO ego
                let nrx = ped.pos_x - other.pos_x;
                let nry = ped.pos_y - other.pos_y;
                let dist = sqrt(nrx * nrx + nry * nry);

                if dist < 1e-6 {
                    // Overlapping -- apply max force in arbitrary direction
                    repulsion_x = repulsion_x + params.max_force;
                    continue;
                }

                let unit_x = nrx / dist;
                let unit_y = nry / dist;

                // Sum of radii
                let r_sum = ped.radius + other.radius;

                // Exponential repulsive force: A * exp((r_sum - d) / B)
                let force_mag = min(params.a_social * exp((r_sum - dist) / params.b_social), params.max_force);

                // Anisotropic weighting
                let cos_phi = -(ego_dir_x * unit_x + ego_dir_y * unit_y);
                let weight = params.lambda + (1.0 - params.lambda) * (1.0 + cos_phi) / 2.0;

                repulsion_x = repulsion_x + weight * force_mag * unit_x;
                repulsion_y = repulsion_y + weight * force_mag * unit_y;
            }
        }
    }

    // Total acceleration
    let accel_x = driving_x + repulsion_x;
    let accel_y = driving_y + repulsion_y;

    // Euler integration with speed clamping
    var new_vx = ped.vel_x + accel_x * params.dt;
    var new_vy = ped.vel_y + accel_y * params.dt;
    let new_speed = sqrt(new_vx * new_vx + new_vy * new_vy);

    if new_speed > params.max_speed && new_speed > 1e-10 {
        let scale = params.max_speed / new_speed;
        new_vx = new_vx * scale;
        new_vy = new_vy * scale;
    }

    // Update position
    ped.pos_x = ped.pos_x + new_vx * params.dt;
    ped.pos_y = ped.pos_y + new_vy * params.dt;
    ped.vel_x = new_vx;
    ped.vel_y = new_vy;

    pedestrians[ped_idx] = ped;
}
