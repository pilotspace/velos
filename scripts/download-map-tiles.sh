#!/usr/bin/env bash
# Download HCMC PMTiles extract from Protomaps global basemap.
#
# Usage: ./scripts/download-map-tiles.sh [--bbox BBOX] [--maxzoom ZOOM]
#
# Defaults to HCMC POC area (Districts 1, 3, 5, 10, Binh Thanh).
# Requires: curl, unzip

set -euo pipefail

BBOX="${1:-106.65,10.74,106.72,10.82}"
MAXZOOM="${2:-15}"
OUTPUT="data/hcmc/hcmc.pmtiles"
PMTILES_VERSION="v1.30.1"
PROTOMAPS_BUILD="20251201"

# Detect platform
OS=$(uname -s)
ARCH=$(uname -m)
case "$OS" in
  Darwin)
    case "$ARCH" in
      arm64) ASSET="go-pmtiles-${PMTILES_VERSION#v}_Darwin_arm64.zip" ;;
      *)     ASSET="go-pmtiles-${PMTILES_VERSION#v}_Darwin_x86_64.zip" ;;
    esac
    ;;
  Linux)
    case "$ARCH" in
      aarch64) ASSET="go-pmtiles_${PMTILES_VERSION#v}_Linux_arm64.tar.gz" ;;
      *)       ASSET="go-pmtiles_${PMTILES_VERSION#v}_Linux_x86_64.tar.gz" ;;
    esac
    ;;
  *) echo "Unsupported OS: $OS"; exit 1 ;;
esac

PMTILES_BIN="/tmp/pmtiles-bin/pmtiles"

# Install pmtiles CLI if not available
if ! command -v pmtiles &>/dev/null && [ ! -x "$PMTILES_BIN" ]; then
  echo "Installing pmtiles CLI ${PMTILES_VERSION}..."
  DOWNLOAD_URL="https://github.com/protomaps/go-pmtiles/releases/download/${PMTILES_VERSION}/${ASSET}"
  mkdir -p /tmp/pmtiles-bin

  if [[ "$ASSET" == *.zip ]]; then
    curl -sL "$DOWNLOAD_URL" -o /tmp/pmtiles.zip
    unzip -o /tmp/pmtiles.zip -d /tmp/pmtiles-bin
  else
    curl -sL "$DOWNLOAD_URL" | tar -xz -C /tmp/pmtiles-bin
  fi
  chmod +x "$PMTILES_BIN"
  echo "Installed: $PMTILES_BIN"
fi

PMTILES_CMD="${PMTILES_BIN}"
command -v pmtiles &>/dev/null && PMTILES_CMD="pmtiles"

# Ensure output directory exists
mkdir -p "$(dirname "$OUTPUT")"

echo "Extracting HCMC tiles from Protomaps basemap (${PROTOMAPS_BUILD})..."
echo "  Bbox: ${BBOX}"
echo "  Max zoom: ${MAXZOOM}"
echo "  Output: ${OUTPUT}"

"$PMTILES_CMD" extract \
  "https://build.protomaps.com/${PROTOMAPS_BUILD}.pmtiles" \
  "$OUTPUT" \
  --bbox="$BBOX" \
  --maxzoom="$MAXZOOM"

echo ""
echo "Done! $(ls -lh "$OUTPUT" | awk '{print $5}') written to ${OUTPUT}"
"$PMTILES_CMD" show "$OUTPUT" 2>&1 | head -8
