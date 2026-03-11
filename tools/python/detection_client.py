"""Python client library for the VELOS DetectionService gRPC API.

Provides a high-level wrapper around the gRPC stubs generated from
proto/velos/v2/detection.proto.

Stub generation command (run from repository root):
    python -m grpc_tools.protoc \
        --proto_path=proto \
        --python_out=tools/python \
        --pyi_out=tools/python \
        --grpc_python_out=tools/python \
        proto/velos/v2/detection.proto

Dependencies:
    uv pip install grpcio grpcio-tools protobuf
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Iterable, Iterator

import grpc

# Generated stubs -- run stub generation command above before importing.
from velos.v2 import detection_pb2
from velos.v2 import detection_pb2_grpc


@dataclass
class CameraInfo:
    """Registered camera information."""

    camera_id: int
    name: str
    lat: float
    lon: float
    heading_deg: float
    fov_deg: float
    range_m: float
    covered_edge_ids: list[int]


def make_detection_event(
    camera_id: int,
    vehicle_class: int,
    count: int,
    speed_kmh: float | None = None,
    timestamp_ms: int = 0,
) -> detection_pb2.DetectionEvent:
    """Create a DetectionEvent protobuf message.

    Args:
        camera_id: Registered camera ID.
        vehicle_class: VehicleClass enum value (1=MOTORBIKE, 2=CAR, 3=BUS,
                       4=TRUCK, 5=BICYCLE, 6=PEDESTRIAN).
        count: Number of detected vehicles of this class.
        speed_kmh: Optional estimated speed in km/h.
        timestamp_ms: Unix epoch timestamp in milliseconds. Defaults to 0
                      (server will use current time if needed).

    Returns:
        A DetectionEvent protobuf message.
    """
    event = detection_pb2.DetectionEvent(
        camera_id=camera_id,
        timestamp_ms=timestamp_ms,
        vehicle_class=vehicle_class,
        count=count,
    )
    if speed_kmh is not None:
        event.speed_kmh = speed_kmh
    return event


class VelosDetectionClient:
    """High-level client for the VELOS DetectionService gRPC API.

    Usage:
        client = VelosDetectionClient("localhost:50051")
        cam_id, edges = client.register_camera(10.7756, 106.7019, 90, 60, 100, "cam-1")
        cameras = client.list_cameras()

        batches = [
            detection_pb2.DetectionBatch(
                batch_id=1,
                events=[make_detection_event(cam_id, 1, 5, speed_kmh=30.0)]
            )
        ]
        for ack in client.stream_detections(batches):
            print(f"Batch {ack.batch_id}: status={ack.status}")
    """

    def __init__(self, addr: str = "localhost:50051") -> None:
        """Connect to a VELOS gRPC detection service.

        Args:
            addr: Server address in host:port format.
        """
        self.channel = grpc.insecure_channel(addr)
        self.stub = detection_pb2_grpc.DetectionServiceStub(self.channel)

    def close(self) -> None:
        """Close the gRPC channel."""
        self.channel.close()

    def __enter__(self) -> VelosDetectionClient:
        return self

    def __exit__(self, *args: object) -> None:
        self.close()

    def register_camera(
        self,
        lat: float,
        lon: float,
        heading_deg: float,
        fov_deg: float,
        range_m: float,
        name: str,
    ) -> tuple[int, list[int]]:
        """Register a camera with the detection service.

        Args:
            lat: Camera latitude (WGS84).
            lon: Camera longitude (WGS84).
            heading_deg: Camera heading in degrees (0=north, clockwise).
            fov_deg: Field of view angle in degrees.
            range_m: Maximum detection range in metres.
            name: Human-readable camera name.

        Returns:
            Tuple of (camera_id, covered_edge_ids).
        """
        request = detection_pb2.RegisterCameraRequest(
            lat=lat,
            lon=lon,
            heading_deg=heading_deg,
            fov_deg=fov_deg,
            range_m=range_m,
            name=name,
        )
        response = self.stub.RegisterCamera(request)
        return response.camera_id, list(response.covered_edge_ids)

    def list_cameras(self) -> list[CameraInfo]:
        """List all registered cameras.

        Returns:
            List of CameraInfo objects.
        """
        response = self.stub.ListCameras(detection_pb2.ListCamerasRequest())
        return [
            CameraInfo(
                camera_id=c.camera_id,
                name=c.name,
                lat=c.lat,
                lon=c.lon,
                heading_deg=c.heading_deg,
                fov_deg=c.fov_deg,
                range_m=c.range_m,
                covered_edge_ids=list(c.covered_edge_ids),
            )
            for c in response.cameras
        ]

    def stream_detections(
        self,
        batches: Iterable[detection_pb2.DetectionBatch],
    ) -> Iterator[detection_pb2.DetectionAck]:
        """Stream detection batches to the server and receive acks.

        This is a bidirectional streaming RPC. The client sends detection
        batches and the server responds with acknowledgments.

        Args:
            batches: Iterable of DetectionBatch messages to stream.

        Yields:
            DetectionAck messages from the server.
        """
        return self.stub.StreamDetections(iter(batches))


def _generate_batches(
    camera_id: int,
    interval: float,
    count_range: tuple[int, int] = (3, 15),
) -> Iterator[detection_pb2.DetectionBatch]:
    """Generate detection batches at a fixed interval.

    Sends motorbike + car detections with randomized counts and speeds.
    Runs until interrupted (Ctrl+C).
    """
    import random
    import time

    batch_id = 1
    while True:
        motorbike_count = random.randint(*count_range)
        car_count = random.randint(1, max(1, count_range[1] // 3))
        ts = int(time.time() * 1000)

        batch = detection_pb2.DetectionBatch(
            batch_id=batch_id,
            events=[
                make_detection_event(
                    camera_id, 1, motorbike_count,
                    speed_kmh=random.uniform(15.0, 45.0),
                    timestamp_ms=ts,
                ),
                make_detection_event(
                    camera_id, 2, car_count,
                    speed_kmh=random.uniform(20.0, 50.0),
                    timestamp_ms=ts,
                ),
            ],
        )
        print(
            f"  batch {batch_id}: {motorbike_count} motorbikes, "
            f"{car_count} cars @ {ts}"
        )
        yield batch
        batch_id += 1
        time.sleep(interval)


if __name__ == "__main__":
    import argparse
    import sys

    parser = argparse.ArgumentParser(
        description="Stream synthetic detections to VELOS gRPC server.",
    )
    parser.add_argument(
        "--addr", default="[::1]:50051",
        help="gRPC server address (default: [::1]:50051)",
    )
    parser.add_argument(
        "--lat", type=float, default=10.7756,
        help="Camera latitude (default: D1 HCMC)",
    )
    parser.add_argument(
        "--lon", type=float, default=106.7019,
        help="Camera longitude (default: D1 HCMC)",
    )
    parser.add_argument(
        "--name", default="test-cam-1",
        help="Camera name (default: test-cam-1)",
    )
    parser.add_argument(
        "--interval", type=float, default=2.0,
        help="Seconds between batches (default: 2.0)",
    )
    args = parser.parse_args()

    print(f"Connecting to {args.addr}...")
    try:
        with VelosDetectionClient(args.addr) as client:
            cam_id, edges = client.register_camera(
                lat=args.lat, lon=args.lon,
                heading_deg=90.0, fov_deg=120.0, range_m=200.0,
                name=args.name,
            )
            print(
                f"Registered camera '{args.name}' -> id={cam_id}, "
                f"edges={edges}"
            )
            print(
                f"Streaming detections every {args.interval}s "
                f"(Ctrl+C to stop)...\n"
            )
            for ack in client.stream_detections(
                _generate_batches(cam_id, args.interval)
            ):
                print(f"  <- ack batch {ack.batch_id}: status={ack.status}")
    except grpc.RpcError as e:
        print(f"gRPC error: {e.code()} - {e.details()}", file=sys.stderr)
        sys.exit(1)
    except KeyboardInterrupt:
        print("\nStopped.")
