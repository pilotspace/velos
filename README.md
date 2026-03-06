# VELOS

GPU-accelerated traffic microsimulation for Ho Chi Minh City.
Simulates motorbikes, cars, and pedestrians using wgpu/Metal on Apple Silicon.

## Build

```sh
cargo build --workspace
```

## Test

```sh
cargo test --workspace
cargo test --workspace --features velos-gpu/gpu-tests   # requires Metal GPU
```

## Architecture

See `docs/architect/` for architecture documentation.
