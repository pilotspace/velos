# VELOS — Self-Hosted Open-Data Tile Map Stack
## Replacing ALL Commercial APIs, SaaS & Paid Services with Open-Source

**Companion to:** `velos-3d-visualization-architecture.md`
**Date:** March 5, 2026

---

## Table of Contents

1. [Commercial Dependency Audit](#1-audit)
2. [Complete Replacement Map](#2-replacement-map)
3. [Self-Hosted Vector Tile Pipeline](#3-vector-tiles)
4. [Self-Hosted 3D Buildings from Open Data](#4-3d-buildings)
5. [Self-Hosted Terrain (DEM)](#5-terrain)
6. [Self-Hosted Satellite & Aerial Imagery](#6-imagery)
7. [Self-Hosted Map Styles & Fonts](#7-styles)
8. [CesiumJS Fully Offline (No Ion)](#8-cesium-offline)
9. [Complete Self-Hosted Stack — Docker Compose](#9-docker-compose)
10. [Per-City Open Data Pipeline](#10-per-city-pipeline)
11. [Open City Data Catalog](#11-city-catalog)
12. [Cost Analysis — Self-Hosted vs Commercial](#12-cost-analysis)
13. [Implementation Guide](#13-implementation)

---

## 1. Commercial Dependency Audit {#1-audit}

Every commercial, SaaS, or API-key-gated service found across all VELOS documents:

| # | Dependency | Where Used | Type | Cost | Risk |
|---|-----------|-----------|------|------|------|
| D1 | **Google Photorealistic 3D Tiles API** | viz-arch §5, §10 | Paid API | $6/1000 requests after free tier | Google can revoke, rate-limit, change pricing |
| D2 | **Cesium Ion** | viz-arch §5, §10 | SaaS | $150-$3000/mo | Hosted tile processing, single vendor lock-in |
| D3 | **Cesium World Terrain** | viz-arch §5 | SaaS | Bundled with Ion | No offline, requires Ion token |
| D4 | **Mapbox GL JS** | viz-arch §6 (legacy ref) | Commercial | Free < 50K loads/mo, then $5/1000 | License changed in v2.0, not truly open |
| D5 | **Mapbox base map tiles** | viz-arch §6 | SaaS | Bundled with Mapbox | Requires API key, usage tracking |
| D6 | **Mapbox terrain** | implied by Mapbox | SaaS | Bundled | No offline |
| D7 | **Google Maps API key** | viz-arch §10 (fallback) | Paid API | $2-$7/1000 requests | Requires billing account |
| D8 | **cdnjs.cloudflare.com** | JS library hosting | CDN (free) | Free but external | Network dependency, not air-gapped |

### Verdict: 7 hard commercial dependencies + 1 soft CDN dependency to eliminate.

---

## 2. Complete Replacement Map {#2-replacement-map}

| Commercial Service | Open-Source Replacement | License | Self-Hosted? | Air-Gap? |
|-------------------|----------------------|---------|:----------:|:-------:|
| D1: Google Photorealistic 3D Tiles | **py3dtilers + CityGML/OSM open data** | Apache 2.0 | Yes | Yes |
| D2: Cesium Ion (tile hosting) | **Nginx/Caddy serving static 3D Tiles** | MIT | Yes | Yes |
| D3: Cesium World Terrain | **SRTM/Mapzen DEM → quantized-mesh tiles** | Public domain | Yes | Yes |
| D4: Mapbox GL JS | **MapLibre GL JS** | BSD-3 | Yes | Yes |
| D5: Mapbox base map tiles | **OpenMapTiles + Martin/tilemaker** | BSD/MIT | Yes | Yes |
| D6: Mapbox terrain | **Self-generated terrain-rgb tiles (rio-rgbify)** | MIT | Yes | Yes |
| D7: Google Maps API | **Removed entirely** | — | — | — |
| D8: cdnjs CDN | **Self-host JS bundles** | Various OSS | Yes | Yes |

**Result: 100% self-hosted, zero API keys, zero SaaS, fully air-gappable.**

---

## 3. Self-Hosted Vector Tile Pipeline {#3-vector-tiles}

Vector tiles provide the base map layer (roads, land use, water, POIs, labels) that everything else sits on top of.

### Tool Comparison

| Tool | Language | Input | Speed | Database? | Best For |
|------|----------|-------|-------|:---------:|---------|
| **Planetiler** | Java | .osm.pbf | Fastest (planet in ~3h) | No | Large regions, production |
| **tilemaker** | C++ | .osm.pbf | Fast | No | Single city, no deps |
| **Martin** | Rust | PostGIS | On-the-fly | Yes | Dynamic queries, flexible |
| **OpenMapTiles** | Docker | .osm.pbf | Medium | Yes | Full schema + styles |
| **Tegola** | Go | PostGIS | Fast | Yes | Lightweight, config-driven |

### Recommended: tilemaker for per-city tiles (simplest, no database)

```bash
# ════════════════════════════════════════════════════
# STEP 1: Download city OSM data (Geofabrik mirrors)
# ════════════════════════════════════════════════════

# Option A: Pre-cut city/region extracts (fast)
wget https://download.geofabrik.de/europe/germany/berlin-latest.osm.pbf
wget https://download.geofabrik.de/europe/germany/bayern/oberbayern-latest.osm.pbf  # Munich
wget https://download.geofabrik.de/asia/vietnam-latest.osm.pbf
wget https://download.geofabrik.de/europe/austria-latest.osm.pbf  # Vienna

# Option B: Cut custom bounding box with osmium
sudo apt install osmium-tool
osmium extract -b 11.4,48.0,11.8,48.3 germany-latest.osm.pbf -o munich.osm.pbf

# Option C: Overpass API for small areas
curl -o area.osm "https://overpass-api.de/api/map?bbox=11.4,48.0,11.8,48.3"

# ════════════════════════════════════════════════════
# STEP 2: Generate vector tiles with tilemaker
# ════════════════════════════════════════════════════

# Install tilemaker (Ubuntu/Debian)
sudo apt install tilemaker
# Or build from source:
git clone https://github.com/systemed/tilemaker.git
cd tilemaker && make && sudo make install

# Generate MBTiles (using OpenMapTiles-compatible schema)
tilemaker \
  --input berlin-latest.osm.pbf \
  --output berlin.mbtiles \
  --config resources/config-openmaptiles.json \
  --process resources/process-openmaptiles.lua \
  --store /tmp/tilemaker-store

# Output: berlin.mbtiles (~200-500 MB for a major city)

# ════════════════════════════════════════════════════
# STEP 3: Serve tiles
# ════════════════════════════════════════════════════

# Option A: Martin (Rust, high-performance)
cargo install martin
martin berlin.mbtiles --listen-addresses 0.0.0.0:3000
# Tiles at: http://localhost:3000/berlin/{z}/{x}/{y}

# Option B: TileServer GL (includes preview + style rendering)
docker run -p 8080:8080 -v $(pwd):/data maptiler/tileserver-gl \
  --mbtiles /data/berlin.mbtiles
# Preview at: http://localhost:8080

# Option C: PMTiles (single-file, no server needed, just Nginx/S3)
# Convert MBTiles → PMTiles for serverless hosting
pip install pmtiles
pmtiles convert berlin.mbtiles berlin.pmtiles
# Serve with any static file server + range requests
```

### Alternative: Planetiler for Multi-City / Country-Scale

```bash
# Download Planetiler
wget https://github.com/onthegomap/planetiler/releases/latest/download/planetiler.jar

# Generate tiles for a single city extract
java -Xmx4g -jar planetiler.jar \
  --osm-path=berlin-latest.osm.pbf \
  --output=berlin.mbtiles \
  --nodemap-type=array \
  --download

# Generate tiles for entire planet (~130GB output)
java -Xmx32g -jar planetiler.jar \
  --download \
  --output=planet.mbtiles \
  --threads=16

# Performance: ~40 CPU-hours for full planet
# Berlin city: ~2 minutes on modern hardware
```

### Alternative: Martin + PostGIS for Dynamic Tiles

```bash
# ════════════════════════════════════════════════════
# Import OSM into PostGIS with osm2pgsql
# ════════════════════════════════════════════════════

# Install osm2pgsql
sudo apt install osm2pgsql

# Create database
createdb osm
psql -d osm -c "CREATE EXTENSION postgis;"
psql -d osm -c "CREATE EXTENSION hstore;"

# Import
osm2pgsql -d osm -C 4000 -s --flat-nodes /tmp/nodes \
  berlin-latest.osm.pbf

# ════════════════════════════════════════════════════
# Configure Martin
# ════════════════════════════════════════════════════

# martin.yaml
cat > martin.yaml << 'EOF'
postgres:
  connection_string: postgresql://localhost/osm
  auto_publish:
    tables: true
    functions: true
  tables:
    planet_osm_polygon:
      geometry_column: way
      srid: 3857
      bounds: [11.4, 48.0, 11.8, 48.3]
    planet_osm_line:
      geometry_column: way
      srid: 3857
    planet_osm_roads:
      geometry_column: way
      srid: 3857
EOF

martin --config martin.yaml --listen-addresses 0.0.0.0:3000
```

---

## 4. Self-Hosted 3D Buildings from Open Data {#4-3d-buildings}

This replaces **Google Photorealistic 3D Tiles** and **Cesium Ion tile processing**.

### Pipeline A: CityGML Open Data → 3D Tiles (highest quality)

Many cities publish free CityGML datasets (LOD1-LOD3). This is the highest-quality path.

```bash
# ════════════════════════════════════════════════════
# STEP 1: Download CityGML from city open data portal
# ════════════════════════════════════════════════════

# Berlin (LOD2, textured, free)
wget "https://daten.berlin.de/datensaetze/3d-stadtmodell-berlin-lod2"
# Direct FTP: ftp://download-berlin3d.virtualcitymap.de/citygml/

# Helsinki (LOD2, free)
# https://kartta.hel.fi/3d/

# Hamburg (LOD1+LOD2, free)
# https://transparenz.hamburg.de/

# Vienna (LOD2, free)
# https://www.data.gv.at/katalog/dataset/3d-gebaudemodell

# NYC (building footprints + height, free)
# https://data.cityofnewyork.us/Housing-Development/Building-Footprints/nqwf-w8eh

# ════════════════════════════════════════════════════
# STEP 2: Optional preprocessing with citygml-tools
# ════════════════════════════════════════════════════

# Install citygml-tools
wget https://github.com/citygml4j/citygml-tools/releases/latest/download/citygml-tools.zip
unzip citygml-tools.zip

# Validate
./citygml-tools validate berlin_lod2.gml

# Upgrade CityGML 2.0 → 3.0 (if needed)
./citygml-tools upgrade --from-version 2.0 berlin_lod2.gml

# Convert to CityJSON (lighter, easier to process)
./citygml-tools to-cityjson berlin_lod2.gml --output berlin.city.json

# ════════════════════════════════════════════════════
# STEP 3: Convert to 3D Tiles with py3dtilers
# ════════════════════════════════════════════════════

pip install py3dtilers --break-system-packages

# CityGML → 3D Tiles
citygml_tiler \
  --input berlin_lod2.gml \
  --output ./3dtiles/berlin/ \
  --srs_in EPSG:25833 \
  --srs_out EPSG:4978 \
  --lod 2 \
  --with_texture

# CityJSON → 3D Tiles
cityjson_tiler \
  --input berlin.city.json \
  --output ./3dtiles/berlin/

# Output structure:
# ./3dtiles/berlin/
# ├── tileset.json          ← root file loaded by CesiumJS
# ├── r.b3dm                ← root tile (Batched 3D Model)
# ├── r/
# │   ├── 0.b3dm           ← child tiles (LOD hierarchy)
# │   ├── 1.b3dm
# │   ├── 2.b3dm
# │   └── 3.b3dm
# └── ...

# ════════════════════════════════════════════════════
# STEP 4: Serve with Nginx (static files, no backend needed)
# ════════════════════════════════════════════════════

# Nginx config for 3D Tiles (CORS + gzip + caching)
cat > /etc/nginx/conf.d/3dtiles.conf << 'NGINX'
server {
    listen 8080;
    server_name localhost;

    location /3dtiles/ {
        alias /var/www/3dtiles/;
        add_header Access-Control-Allow-Origin *;
        add_header Access-Control-Allow-Methods "GET, OPTIONS";

        # Gzip for JSON (tileset.json)
        gzip on;
        gzip_types application/json application/octet-stream;

        # Cache static tiles aggressively
        expires 30d;
        add_header Cache-Control "public, immutable";

        # Correct MIME types
        types {
            application/json json;
            application/octet-stream b3dm pnts i3dm cmpt;
            model/gltf-binary glb;
            model/gltf+json gltf;
        }
    }
}
NGINX

nginx -t && nginx -s reload
# 3D Tiles at: http://localhost:8080/3dtiles/berlin/tileset.json
```

### Pipeline B: OSM Building Footprints → 3D Tiles (any city, no CityGML needed)

For cities without CityGML data, generate 3D buildings from OSM footprints.

```bash
# ════════════════════════════════════════════════════
# STEP 1: Extract building footprints from OSM
# ════════════════════════════════════════════════════

# Method A: osmium + ogr2ogr
osmium tags-filter munich.osm.pbf w/building -o munich-buildings.osm.pbf
ogr2ogr -f GeoJSON buildings.geojson munich-buildings.osm.pbf multipolygons

# Method B: Python with osmnx
pip install osmnx geopandas --break-system-packages

python3 << 'PYEOF'
import osmnx as ox
import geopandas as gpd

# Download building footprints for any city
place = "Ho Chi Minh City, Vietnam"
buildings = ox.features_from_place(place, tags={"building": True})

# OSM building tags include:
#   building:levels → multiply by 3m for height
#   height → direct height in meters
#   building:height → alternative tag

def estimate_height(row):
    """Estimate building height from OSM tags."""
    if 'height' in row and row['height']:
        try:
            return float(str(row['height']).replace('m', '').strip())
        except (ValueError, TypeError):
            pass
    if 'building:levels' in row and row['building:levels']:
        try:
            return float(row['building:levels']) * 3.0  # 3m per floor
        except (ValueError, TypeError):
            pass
    # Default heights by building type
    defaults = {
        'apartments': 15.0,
        'commercial': 12.0,
        'industrial': 8.0,
        'residential': 9.0,
        'house': 7.0,
        'retail': 5.0,
        'yes': 9.0,  # generic
    }
    btype = str(row.get('building', 'yes'))
    return defaults.get(btype, 9.0)

buildings['estimated_height'] = buildings.apply(estimate_height, axis=1)

# Save as GeoJSON with height
buildings[['geometry', 'estimated_height', 'building']].to_file(
    'hcmc_buildings.geojson', driver='GeoJSON'
)
print(f"Exported {len(buildings)} buildings")
PYEOF

# ════════════════════════════════════════════════════
# STEP 2: Convert footprints to 3D Tiles
# ════════════════════════════════════════════════════

# Option A: py3dtilers with GeoJSON
geojson_tiler \
  --input hcmc_buildings.geojson \
  --output ./3dtiles/hcmc/ \
  --height_attribute estimated_height

# Option B: Custom Python with py3dtiles
python3 << 'PYEOF'
import json
import numpy as np
from py3dtiles.tileset import TileSet
from py3dtiles.tile import Tile
from py3dtiles.batch_table import BatchTable
from py3dtiles.feature_table import FeatureTable

# Load GeoJSON
with open('hcmc_buildings.geojson') as f:
    geojson = json.load(f)

# Process each building → glTF → b3dm
# ... (simplified — py3dtilers handles this automatically)
PYEOF

# ════════════════════════════════════════════════════
# STEP 3: Alternative — osm2world for full 3D scene
# ════════════════════════════════════════════════════
# ⚠️ LICENSE WARNING: osm2world is GPLv3.
# Do NOT integrate as a library into VELOS (proprietary).
# SAFE USAGE: run as a standalone CLI tool in a build pipeline.
# The GPL applies to osm2world itself, NOT to its output files.
# Using it as an external tool that produces glTF output is fine.

# osm2world generates buildings, roads, vegetation, terrain
wget https://osm2world.org/download/osm2world-latest.jar

java -jar osm2world-latest.jar \
  --input munich.osm.pbf \
  --output munich_3d.gltf \
  --config osm2world.properties

# Convert glTF → 3D Tiles
npm install -g 3d-tiles-tools gltf-pipeline

# Optimize glTF
gltf-pipeline -i munich_3d.gltf -o munich_3d.glb --draco.compressionLevel 7

# Create tileset.json manually (for single-tile simple case)
cat > ./3dtiles/munich/tileset.json << 'TILESET'
{
  "asset": { "version": "1.0" },
  "geometricError": 500,
  "root": {
    "boundingVolume": {
      "region": [0.1989, 0.8378, 0.2059, 0.8431, 0, 600]
    },
    "geometricError": 100,
    "content": { "uri": "munich_3d.glb" },
    "refine": "REPLACE"
  }
}
TILESET
```

### Pipeline C: LiDAR Point Cloud → 3D Tiles

```bash
# For cities with open LiDAR data (NYC, Netherlands, Switzerland, etc.)

# Convert LAS/LAZ → 3D Tiles (.pnts format)
pip install py3dtiles[las] --break-system-packages

py3dtiles convert \
  --srs_in EPSG:2263 \
  --srs_out EPSG:4978 \
  --out ./3dtiles/nyc-lidar/ \
  nyc_lidar_2017.las

# Or with PDAL (Point Data Abstraction Library)
sudo apt install pdal

pdal pipeline << 'JSON'
{
  "pipeline": [
    { "type": "readers.las", "filename": "nyc_lidar.las" },
    { "type": "filters.reprojection",
      "in_srs": "EPSG:2263", "out_srs": "EPSG:4978" },
    { "type": "writers.cesiumtiles",
      "output_dir": "./3dtiles/nyc-lidar/" }
  ]
}
JSON
```

---

## 5. Self-Hosted Terrain (DEM) {#5-terrain}

Replaces **Cesium World Terrain** and **Mapbox Terrain**.

### Data Sources (ALL FREE)

| Source | Resolution | Coverage | Format | Download |
|--------|-----------|----------|--------|----------|
| **SRTM v3** (NASA) | 30m | Global (60°N-56°S) | GeoTIFF | [USGS EarthExplorer](https://earthexplorer.usgs.gov/) |
| **ALOS AW3D30** (JAXA) | 30m | Global | GeoTIFF | [JAXA Portal](https://www.eorc.jaxa.jp/ALOS/en/dataset/aw3d30/) |
| **Copernicus GLO-30** | 30m | Global | GeoTIFF | [Copernicus Data Space](https://dataspace.copernicus.eu/) |
| **EU-DEM v1.1** | 25m | Europe | GeoTIFF | [Copernicus Land](https://land.copernicus.eu/) |
| **Mapzen Terrain Tiles** | ~30m | Global | Terrarium/GeoTIFF | [AWS OpenData](https://registry.opendata.aws/terrain-tiles/) (free) |
| **ASTER GDEM v3** | 30m | Global | GeoTIFF | [NASA LP DAAC](https://lpdaac.usgs.gov/) |
| **Local LiDAR DEMs** | 1-5m | City-specific | GeoTIFF | City open data portals |

### Generate Terrain-RGB Tiles (for MapLibre GL JS)

```bash
# ════════════════════════════════════════════════════
# STEP 1: Download DEM data
# ════════════════════════════════════════════════════

# Option A: Mapzen terrain tiles from AWS (pre-tiled, easiest)
# These are already tiled — just mirror the ones you need
pip install mercantile requests --break-system-packages

python3 << 'PYEOF'
import mercantile
import requests
import os

# Define city bounding box (Munich example)
bbox = (11.4, 48.0, 11.8, 48.3)  # west, south, east, north
zoom_levels = range(0, 15)

for z in zoom_levels:
    tiles = list(mercantile.tiles(*bbox, zooms=z))
    print(f"Zoom {z}: {len(tiles)} tiles")
    for tile in tiles:
        url = f"https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{tile.z}/{tile.x}/{tile.y}.png"
        out_dir = f"terrain/{tile.z}/{tile.x}"
        os.makedirs(out_dir, exist_ok=True)
        out_path = f"{out_dir}/{tile.y}.png"
        if not os.path.exists(out_path):
            r = requests.get(url)
            if r.status_code == 200:
                with open(out_path, 'wb') as f:
                    f.write(r.content)

print("Done! Terrain tiles saved to ./terrain/")
PYEOF

# Option B: Generate from raw DEM with GDAL + rio-rgbify
sudo apt install gdal-bin python3-gdal
pip install rasterio rio-rgbify --break-system-packages

# Download SRTM tile (e.g., N48E011 for Munich)
wget https://e4ftl01.cr.usgs.gov/MEASURES/SRTMGL1.003/2000.02.11/N48E011.SRTMGL1.hgt.zip
unzip N48E011.SRTMGL1.hgt.zip

# Merge multiple SRTM tiles if needed
gdal_merge.py -o merged_dem.tif N48E011.hgt N48E012.hgt N49E011.hgt N49E012.hgt

# Reproject to Web Mercator
gdalwarp -t_srs EPSG:3857 -r bilinear merged_dem.tif dem_3857.tif

# Convert to Terrain-RGB (Mapzen terrarium encoding)
rio rgbify \
  --base-val -10000 \
  --interval 0.1 \
  --format png \
  dem_3857.tif \
  terrain_rgb.mbtiles

# Extract to directory structure for static serving
mb-util terrain_rgb.mbtiles terrain_tiles/ --image_format=png

# ════════════════════════════════════════════════════
# STEP 2: Serve terrain tiles
# ════════════════════════════════════════════════════

# Static file server (Nginx/Caddy)
# Files are at: terrain_tiles/{z}/{x}/{y}.png
# Serve from: http://localhost:8080/terrain/{z}/{x}/{y}.png
```

### Generate Quantized-Mesh Terrain (for CesiumJS)

CesiumJS uses quantized-mesh format (not terrain-rgb). Use `ctb-tile` to generate:

```bash
# ════════════════════════════════════════════════════
# Cesium Terrain Builder (ctb-tile)
# ════════════════════════════════════════════════════

# Install via Docker
docker pull tumgis/ctb-quantized-mesh

# Generate quantized-mesh tiles from DEM
docker run --rm -v $(pwd):/data tumgis/ctb-quantized-mesh \
  ctb-tile -f Mesh -C -N -o /data/terrain_cesium /data/merged_dem.tif

# Generate layer.json (metadata file CesiumJS needs)
docker run --rm -v $(pwd):/data tumgis/ctb-quantized-mesh \
  ctb-tile -f Mesh -C -N -l -o /data/terrain_cesium /data/merged_dem.tif

# Output structure:
# terrain_cesium/
# ├── layer.json          ← metadata for CesiumJS
# ├── 0/
# │   └── 0/
# │       └── 0.terrain   ← quantized-mesh tile
# ├── 1/
# │   ├── 0/
# │   └── 1/
# └── ...

# Serve with Nginx (same as 3D Tiles)
# CesiumJS loads: http://localhost:8080/terrain_cesium/layer.json
```

### Use in MapLibre GL JS

```javascript
// MapLibre GL JS with self-hosted terrain
const map = new maplibregl.Map({
    container: 'map',
    style: {
        version: 8,
        sources: {
            'osm-tiles': {
                type: 'vector',
                url: 'http://localhost:3000/berlin'  // Martin tile server
            },
            'terrain-source': {
                type: 'raster-dem',
                tiles: ['http://localhost:8080/terrain/{z}/{x}/{y}.png'],
                tileSize: 256,
                encoding: 'terrarium',  // Mapzen encoding
                maxzoom: 14
            }
        },
        terrain: {
            source: 'terrain-source',
            exaggeration: 1.0
        },
        layers: [/* ... */]
    },
    center: [11.58, 48.15],
    zoom: 13,
    pitch: 45
});
```

### Use in CesiumJS (self-hosted)

```javascript
// CesiumJS with self-hosted quantized-mesh terrain
const viewer = new Cesium.Viewer('cesiumContainer', {
    terrainProvider: await Cesium.CesiumTerrainProvider.fromUrl(
        'http://localhost:8080/terrain_cesium/',
        { requestVertexNormals: true }
    ),
    // NO Ion token needed!
});
```

---

## 6. Self-Hosted Satellite & Aerial Imagery {#6-imagery}

Replaces Mapbox satellite tiles and Cesium Ion imagery.

### Data Sources (ALL FREE)

| Source | Resolution | Revisit | Coverage | Access |
|--------|-----------|---------|----------|--------|
| **Sentinel-2** (ESA) | 10m | 5 days | Global | [Copernicus Data Space](https://dataspace.copernicus.eu/) |
| **Landsat 8/9** (USGS) | 30m | 16 days | Global | [USGS EarthExplorer](https://earthexplorer.usgs.gov/) |
| **NAIP** (USDA) | 1m | 2-3 years | USA | [AWS OpenData](https://registry.opendata.aws/naip/) |
| **OpenAerialMap** | Varies | Community | Scattered | [openaerialmap.org](https://openaerialmap.org/) |

### Generate Raster Tiles from Satellite Imagery

```bash
# ════════════════════════════════════════════════════
# Using GDAL to create raster tiles from GeoTIFF
# ════════════════════════════════════════════════════

# Download Sentinel-2 scene (use Copernicus Data Space API)
# Or download via sentinelsat Python library:
pip install sentinelsat --break-system-packages

python3 << 'PYEOF'
from sentinelsat import SentinelAPI
from datetime import date

# Connect to Copernicus (free registration)
api = SentinelAPI('username', 'password', 'https://scihub.copernicus.eu/dhus')

# Search for Sentinel-2 imagery
footprint = "POLYGON((11.4 48.0, 11.8 48.0, 11.8 48.3, 11.4 48.3, 11.4 48.0))"
products = api.query(
    footprint,
    date=('20250101', '20250301'),
    platformname='Sentinel-2',
    cloudcoverpercentage=(0, 10),
    processinglevel='Level-2A'
)

# Download best scene
api.download(list(products.keys())[0])
PYEOF

# Create true-color composite (B4, B3, B2 → RGB)
gdal_merge.py -separate -o rgb.tif B04.jp2 B03.jp2 B02.jp2

# Reproject to Web Mercator
gdalwarp -t_srs EPSG:3857 rgb.tif rgb_3857.tif

# Generate tile pyramid
gdal2tiles.py -z 8-16 -w none --xyz rgb_3857.tif ./satellite_tiles/

# Serve with Nginx
# Access at: http://localhost:8080/satellite/{z}/{x}/{y}.png
```

---

## 7. Self-Hosted Map Styles & Fonts {#7-styles}

Replaces Mapbox styles and Google Maps styling.

### MapLibre GL JS — Complete Self-Hosted Setup

```bash
# ════════════════════════════════════════════════════
# STEP 1: Download open map fonts (glyphs)
# ════════════════════════════════════════════════════

# OpenMapTiles fonts (Noto Sans, Roboto, etc.)
git clone https://github.com/openmaptiles/fonts.git
cd fonts && node generate.js  # generates PBF glyph ranges

# Or download pre-built from MapTiler
# Place in: ./fonts/{fontstack}/{range}.pbf

# ════════════════════════════════════════════════════
# STEP 2: Download/create sprite sheets (icons)
# ════════════════════════════════════════════════════

# OpenMapTiles sprites
git clone https://github.com/openmaptiles/openmaptiles-sprites.git
# Or use Maki icons from Mapbox (CC0 license)
npm install @mapbox/maki
# Generate sprite: spritesmith or spritezero-cli

# ════════════════════════════════════════════════════
# STEP 3: Create self-hosted style JSON
# ════════════════════════════════════════════════════

cat > style-velos.json << 'STYLE'
{
  "version": 8,
  "name": "VELOS Dark",
  "sources": {
    "openmaptiles": {
      "type": "vector",
      "url": "http://localhost:3000/berlin"
    },
    "terrain": {
      "type": "raster-dem",
      "tiles": ["http://localhost:8080/terrain/{z}/{x}/{y}.png"],
      "tileSize": 256,
      "encoding": "terrarium"
    }
  },
  "sprite": "http://localhost:8080/styles/sprites/velos",
  "glyphs": "http://localhost:8080/fonts/{fontstack}/{range}.pbf",
  "terrain": {
    "source": "terrain",
    "exaggeration": 1.0
  },
  "layers": [
    {
      "id": "background",
      "type": "background",
      "paint": { "background-color": "#1a1a2e" }
    },
    {
      "id": "water",
      "type": "fill",
      "source": "openmaptiles",
      "source-layer": "water",
      "paint": { "fill-color": "#0a1628" }
    },
    {
      "id": "landuse-park",
      "type": "fill",
      "source": "openmaptiles",
      "source-layer": "landuse",
      "filter": ["==", "class", "park"],
      "paint": { "fill-color": "#1a3a1a", "fill-opacity": 0.6 }
    },
    {
      "id": "roads-highway",
      "type": "line",
      "source": "openmaptiles",
      "source-layer": "transportation",
      "filter": ["==", "class", "motorway"],
      "paint": {
        "line-color": "#3a3a5e",
        "line-width": ["interpolate", ["exponential", 1.5], ["zoom"], 5, 0.5, 18, 20]
      }
    },
    {
      "id": "roads-main",
      "type": "line",
      "source": "openmaptiles",
      "source-layer": "transportation",
      "filter": ["in", "class", "primary", "secondary", "tertiary"],
      "paint": {
        "line-color": "#2a2a4e",
        "line-width": ["interpolate", ["exponential", 1.5], ["zoom"], 8, 0.5, 18, 12]
      }
    },
    {
      "id": "buildings-3d",
      "type": "fill-extrusion",
      "source": "openmaptiles",
      "source-layer": "building",
      "paint": {
        "fill-extrusion-color": "#2a2a3e",
        "fill-extrusion-height": ["coalesce", ["get", "render_height"], 10],
        "fill-extrusion-base": ["coalesce", ["get", "render_min_height"], 0],
        "fill-extrusion-opacity": 0.7
      }
    },
    {
      "id": "place-labels",
      "type": "symbol",
      "source": "openmaptiles",
      "source-layer": "place",
      "layout": {
        "text-field": "{name}",
        "text-font": ["Noto Sans Regular"],
        "text-size": 14
      },
      "paint": {
        "text-color": "#aaaacc",
        "text-halo-color": "#1a1a2e",
        "text-halo-width": 1
      }
    }
  ]
}
STYLE

# ════════════════════════════════════════════════════
# STEP 4: Or use pre-built open styles
# ════════════════════════════════════════════════════

# OSM Liberty (recommended, MapLibre-native)
wget https://raw.githubusercontent.com/maputnik/osm-liberty/gh-pages/style.json
# Edit to point sources at your self-hosted tile server

# Positron / Dark Matter (CartoDB styles, open)
# Available at: https://github.com/openmaptiles/positron-gl-style
# Available at: https://github.com/openmaptiles/dark-matter-gl-style
```

### Style Editor — Maputnik (self-hosted)

```bash
# Visual map style editor (runs in browser, fully local)
docker run -p 8888:8888 maputnik/editor
# Open: http://localhost:8888
# Load your style JSON, edit visually, export
```

---

## 8. CesiumJS Fully Offline (No Ion) {#8-cesium-offline}

Complete CesiumJS setup with ZERO external dependencies.

```javascript
// velos-cesium-offline.js
// ═══════════════════════════════════════════════════════
// CesiumJS initialized WITHOUT Cesium Ion
// All data served from self-hosted infrastructure
// ═══════════════════════════════════════════════════════

// IMPORTANT: Do NOT set Cesium.Ion.defaultAccessToken
// This ensures no requests go to Cesium Ion

const viewer = new Cesium.Viewer('cesiumContainer', {
    // ─── Disable all Ion-dependent features ───
    imageryProvider: false,        // We'll add our own
    terrain: undefined,            // We'll add our own
    geocoder: false,               // Requires Ion
    baseLayerPicker: false,        // Ion imagery list
    skyBox: false,                 // Optional: add custom skybox
    skyAtmosphere: true,           // Works without Ion
    timeline: false,
    animation: false,
    homeButton: false,
    sceneModePicker: false,
    navigationHelpButton: false,
});

// ─── Self-hosted base imagery (OSM raster tiles) ───
const osmImagery = new Cesium.OpenStreetMapImageryProvider({
    url: 'http://localhost:8080/satellite/',  // Self-hosted raster tiles
    // Or use free OSM tile servers (no API key):
    // url: 'https://tile.openstreetmap.org/'
});
viewer.imageryLayers.addImageryProvider(osmImagery);

// Alternative: TMS tiles from self-hosted TileServer GL
const tmsImagery = new Cesium.TileMapServiceImageryProvider({
    url: 'http://localhost:8081/styles/osm-bright/',
});
viewer.imageryLayers.addImageryProvider(tmsImagery);

// ─── Self-hosted terrain (quantized-mesh) ───
const terrainProvider = await Cesium.CesiumTerrainProvider.fromUrl(
    'http://localhost:8080/terrain_cesium/',
    {
        requestVertexNormals: true,
        requestWaterMask: false,
    }
);
viewer.terrainProvider = terrainProvider;

// ─── Self-hosted 3D Tiles (buildings) ───
const buildingTileset = await Cesium.Cesium3DTileset.fromUrl(
    'http://localhost:8080/3dtiles/berlin/tileset.json',
    {
        maximumScreenSpaceError: 8,
        maximumMemoryUsage: 512,        // MB
        skipLevelOfDetail: false,
        dynamicScreenSpaceError: true,
        dynamicScreenSpaceErrorDensity: 0.00278,
        dynamicScreenSpaceErrorFactor: 4.0,
    }
);
viewer.scene.primitives.add(buildingTileset);

// ─── Style 3D Tiles (dark theme matching VELOS) ───
buildingTileset.style = new Cesium.Cesium3DTileStyle({
    color: {
        conditions: [
            ["${height} > 50", "color('#4a4a6e')"],
            ["${height} > 20", "color('#3a3a5e')"],
            ["true", "color('#2a2a4e')"],
        ],
    },
});

// ─── VELOS agent overlay (WebSocket, same as before) ───
// No changes needed — the agent streaming is already self-hosted

// ─── Camera: fly to city ───
viewer.camera.flyTo({
    destination: Cesium.Cartesian3.fromDegrees(13.405, 52.52, 2000),
    orientation: {
        heading: Cesium.Math.toRadians(0),
        pitch: Cesium.Math.toRadians(-45),
        roll: 0,
    },
});

console.log('CesiumJS running fully offline — zero external API calls');
```

### Self-Host CesiumJS Library Itself

```bash
# Download CesiumJS release (no CDN dependency)
wget https://github.com/CesiumGS/cesium/releases/download/1.113/Cesium-1.113.zip
unzip Cesium-1.113.zip -d /var/www/html/cesium/

# Also bundle: MapLibre GL JS, deck.gl, FlatBuffers
npm pack maplibre-gl @deck.gl/core @deck.gl/layers flatbuffers
# Extract and serve from /var/www/html/js/

# Now ALL JavaScript loads from localhost — fully air-gappable
```

---

## 9. Complete Self-Hosted Stack — Docker Compose {#9-docker-compose}

Single `docker-compose.yml` that runs the entire VELOS visualization infrastructure:

```yaml
# docker-compose.yml — VELOS Self-Hosted Map Stack
# Zero commercial APIs. Zero SaaS. Fully air-gappable.

version: '3.8'

services:
  # ─────────────────────────────────────────────────
  # PostgreSQL + PostGIS (spatial database)
  # ─────────────────────────────────────────────────
  postgres:
    image: postgis/postgis:16-3.4
    environment:
      POSTGRES_PASSWORD: velos
      POSTGRES_DB: osm
    ports:
      - "5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data
    healthcheck:
      test: pg_isready -U postgres
      interval: 10s
      timeout: 5s
      retries: 5

  # ─────────────────────────────────────────────────
  # Martin — Rust vector tile server (from PostGIS)
  # ─────────────────────────────────────────────────
  martin:
    image: ghcr.io/maplibre/martin:latest
    ports:
      - "3000:3000"
    environment:
      DATABASE_URL: postgresql://postgres:velos@postgres/osm
    depends_on:
      postgres:
        condition: service_healthy
    command: ["--listen-addresses", "0.0.0.0:3000"]

  # ─────────────────────────────────────────────────
  # Static file server (3D Tiles, terrain, fonts, sprites)
  # ─────────────────────────────────────────────────
  static:
    image: nginx:alpine
    ports:
      - "8080:80"
    volumes:
      - ./data/3dtiles:/usr/share/nginx/html/3dtiles:ro
      - ./data/terrain:/usr/share/nginx/html/terrain:ro
      - ./data/terrain_cesium:/usr/share/nginx/html/terrain_cesium:ro
      - ./data/satellite:/usr/share/nginx/html/satellite:ro
      - ./data/fonts:/usr/share/nginx/html/fonts:ro
      - ./data/sprites:/usr/share/nginx/html/sprites:ro
      - ./data/styles:/usr/share/nginx/html/styles:ro
      - ./data/js:/usr/share/nginx/html/js:ro        # self-hosted JS libs
      - ./nginx.conf:/etc/nginx/conf.d/default.conf:ro

  # ─────────────────────────────────────────────────
  # TileServer GL — raster tile rendering + preview
  # ─────────────────────────────────────────────────
  tileserver:
    image: maptiler/tileserver-gl:latest
    ports:
      - "8081:80"
    volumes:
      - ./data/mbtiles:/data
    command: ["--config", "/data/config.json"]

  # ─────────────────────────────────────────────────
  # Maputnik — visual style editor (dev only)
  # ─────────────────────────────────────────────────
  maputnik:
    image: maputnik/editor:latest
    ports:
      - "8888:8888"
    profiles:
      - dev    # Only start with: docker compose --profile dev up

  # ─────────────────────────────────────────────────
  # VELOS Engine (placeholder — actual sim engine)
  # ─────────────────────────────────────────────────
  # velos:
  #   build: ./velos-engine
  #   ports:
  #     - "9090:9090"     # gRPC
  #     - "9091:9091"     # WebSocket viz stream
  #   depends_on:
  #     - postgres

volumes:
  postgres_data:
```

### Nginx Configuration

```nginx
# nginx.conf — serves 3D Tiles, terrain, static assets
server {
    listen 80;
    server_name localhost;

    # Enable CORS for all origins (VELOS web clients)
    add_header Access-Control-Allow-Origin * always;
    add_header Access-Control-Allow-Methods "GET, HEAD, OPTIONS" always;
    add_header Access-Control-Allow-Headers "Range, Accept-Encoding" always;
    add_header Access-Control-Expose-Headers "Content-Length, Content-Range" always;

    # Gzip compression
    gzip on;
    gzip_types application/json application/javascript text/css
               application/octet-stream model/gltf+json;
    gzip_min_length 256;

    # ─── 3D Tiles (buildings) ───
    location /3dtiles/ {
        alias /usr/share/nginx/html/3dtiles/;
        expires 30d;
        add_header Cache-Control "public, immutable";
        types {
            application/json json;
            application/octet-stream b3dm pnts i3dm cmpt glb terrain;
            model/gltf-binary glb;
            model/gltf+json gltf;
        }
    }

    # ─── Terrain tiles (terrain-rgb PNG or quantized-mesh) ───
    location /terrain/ {
        alias /usr/share/nginx/html/terrain/;
        expires 30d;
    }

    location /terrain_cesium/ {
        alias /usr/share/nginx/html/terrain_cesium/;
        expires 30d;
        types {
            application/json json;
            application/octet-stream terrain;
            application/vnd.quantized-mesh terrain;
        }
    }

    # ─── Satellite imagery tiles ───
    location /satellite/ {
        alias /usr/share/nginx/html/satellite/;
        expires 7d;
    }

    # ─── Fonts (PBF glyphs) ───
    location /fonts/ {
        alias /usr/share/nginx/html/fonts/;
        expires 365d;
        types {
            application/x-protobuf pbf;
        }
    }

    # ─── Sprites (icon atlases) ───
    location /sprites/ {
        alias /usr/share/nginx/html/sprites/;
        expires 365d;
    }

    # ─── Map styles ───
    location /styles/ {
        alias /usr/share/nginx/html/styles/;
        expires 1d;
    }

    # ─── Self-hosted JavaScript libraries ───
    location /js/ {
        alias /usr/share/nginx/html/js/;
        expires 365d;
        add_header Cache-Control "public, immutable";
    }
}
```

---

## 10. Per-City Open Data Pipeline {#10-per-city-pipeline}

### Automated Script: Add a New City to VELOS

```bash
#!/bin/bash
# add-city.sh — Download open data and generate all tiles for a new city
# Usage: ./add-city.sh <city_name> <bbox> [citygml_url]
#
# Example: ./add-city.sh munich "11.4,48.0,11.8,48.3"
# Example: ./add-city.sh berlin "13.2,52.3,13.6,52.7" "ftp://download-berlin3d..."

set -e

CITY=$1
BBOX=$2           # west,south,east,north
CITYGML_URL=$3    # optional: URL to CityGML dataset
DATA_DIR="./data"
WORK_DIR="/tmp/velos-city-${CITY}"

echo "═══════════════════════════════════════"
echo "  Adding city: ${CITY}"
echo "  Bounding box: ${BBOX}"
echo "═══════════════════════════════════════"

mkdir -p ${WORK_DIR} ${DATA_DIR}/{3dtiles,terrain,terrain_cesium,satellite}

# ─── STEP 1: Download OSM data ───
echo "[1/6] Downloading OSM data..."
GEOFABRIK_URL="https://download.geofabrik.de"
# For custom bbox, use osmium extract on country file
# For known regions, download pre-cut extract
wget -q -O ${WORK_DIR}/${CITY}.osm.pbf \
  "${GEOFABRIK_URL}/europe/germany/${CITY}-latest.osm.pbf" 2>/dev/null || \
  echo "Pre-cut not available. Use osmium extract on country file."

# ─── STEP 2: Generate vector tiles ───
echo "[2/6] Generating vector tiles..."
tilemaker \
  --input ${WORK_DIR}/${CITY}.osm.pbf \
  --output ${DATA_DIR}/mbtiles/${CITY}.mbtiles \
  --config resources/config-openmaptiles.json \
  --process resources/process-openmaptiles.lua

# ─── STEP 3: Generate 3D buildings ───
echo "[3/6] Generating 3D buildings..."
if [ -n "${CITYGML_URL}" ]; then
    # Use CityGML if available
    wget -q -O ${WORK_DIR}/${CITY}_citygml.gml "${CITYGML_URL}"
    citygml_tiler \
      --input ${WORK_DIR}/${CITY}_citygml.gml \
      --output ${DATA_DIR}/3dtiles/${CITY}/ \
      --with_texture
else
    # Generate from OSM building footprints
    python3 scripts/osm_buildings_to_3dtiles.py \
      --input ${WORK_DIR}/${CITY}.osm.pbf \
      --output ${DATA_DIR}/3dtiles/${CITY}/ \
      --bbox ${BBOX}
fi

# ─── STEP 4: Download terrain ───
echo "[4/6] Downloading terrain tiles..."
IFS=',' read -r WEST SOUTH EAST NORTH <<< "${BBOX}"
python3 scripts/download_terrain.py \
  --bbox ${WEST} ${SOUTH} ${EAST} ${NORTH} \
  --output-terrarium ${DATA_DIR}/terrain/ \
  --output-cesium ${DATA_DIR}/terrain_cesium/ \
  --max-zoom 14

# ─── STEP 5: Download satellite imagery ───
echo "[5/6] Downloading satellite imagery..."
python3 scripts/download_sentinel2.py \
  --bbox ${WEST} ${SOUTH} ${EAST} ${NORTH} \
  --output ${DATA_DIR}/satellite/ \
  --max-cloud 10

# ─── STEP 6: Register city in VELOS ───
echo "[6/6] Registering city..."
cat >> ${DATA_DIR}/cities.json << EOF
{
  "name": "${CITY}",
  "bbox": [${BBOX}],
  "vector_tiles": "http://localhost:3000/${CITY}/{z}/{x}/{y}",
  "buildings_3dtiles": "http://localhost:8080/3dtiles/${CITY}/tileset.json",
  "terrain_cesium": "http://localhost:8080/terrain_cesium/layer.json",
  "terrain_rgb": "http://localhost:8080/terrain/{z}/{x}/{y}.png"
}
EOF

echo "═══════════════════════════════════════"
echo "  City '${CITY}' ready!"
echo "  Vector tiles:  localhost:3000/${CITY}/{z}/{x}/{y}"
echo "  3D Tiles:      localhost:8080/3dtiles/${CITY}/tileset.json"
echo "  Terrain:       localhost:8080/terrain_cesium/layer.json"
echo "═══════════════════════════════════════"
```

---

## 11. Open City Data Catalog {#11-city-catalog}

### Cities with Free CityGML Downloads (LOD2+)

| City | Country | LOD | Textured? | Format | Download URL |
|------|---------|-----|:---------:|--------|-------------|
| **Berlin** | Germany | LOD2 | Yes | CityGML | [berlin.de/3d](https://daten.berlin.de/datensaetze/3d-stadtmodell-berlin-lod2) |
| **Hamburg** | Germany | LOD1+2 | No | CityGML | [transparenz.hamburg.de](https://transparenz.hamburg.de/) |
| **Cologne** | Germany | LOD2 | No | CityGML | [offenedaten-koeln.de](https://offenedaten-koeln.de/) |
| **Munich** | Germany | LOD2 | Partial | CityGML | [opendata.muenchen.de](https://opendata.muenchen.de/) |
| **Helsinki** | Finland | LOD2 | Yes | CityGML/OBJ | [kartta.hel.fi/3d](https://kartta.hel.fi/3d/) |
| **Vienna** | Austria | LOD2 | No | CityGML | [data.gv.at](https://www.data.gv.at/) |
| **Zurich** | Switzerland | LOD2 | Yes | CityGML | [Stadt Zürich Open Data](https://data.stadt-zuerich.ch/) |
| **Rotterdam** | Netherlands | LOD2 | No | CityGML | [3dbag.nl](https://3dbag.nl/) |
| **The Hague** | Netherlands | LOD2 | No | CityGML | [3dbag.nl](https://3dbag.nl/) |
| **Amsterdam** | Netherlands | LOD2 | No | CityGML | [3dbag.nl](https://3dbag.nl/) |
| **NYC** | USA | LOD1 | No | SHP + Height | [data.cityofnewyork.us](https://data.cityofnewyork.us/) |
| **Singapore** | Singapore | LOD1 | No | CityJSON | [data.gov.sg](https://data.gov.sg/) |

### Netherlands Special: 3DBAG — Entire Country at LOD2

The Netherlands publishes **every building in the country** as LOD2 CityJSON:

```bash
# Download the entire Netherlands at LOD 1.2 and 2.2
# https://3dbag.nl/en/download
wget https://data.3dbag.nl/v20240220/3dbag_nl_cityjson.zip
# or individual tiles via WFS
```

### Any City: OSM-Derived 3D Buildings

For cities without official CityGML releases, OSM building footprints provide reasonable 3D models. Coverage quality depends on local OSM community activity:

| Region | OSM Building Coverage | Height Data Quality |
|--------|:--------------------:|:------------------:|
| Western Europe | Excellent (>95%) | Good (many have levels tag) |
| North America | Good (>80%) | Moderate |
| East Asia | Good (>80%) | Moderate |
| Southeast Asia | Moderate (>60%) | Limited (mostly defaults) |
| South America | Moderate (>50%) | Limited |
| Africa | Variable (20-80%) | Limited |

---

## 12. Cost Analysis — Self-Hosted vs Commercial {#12-cost-analysis}

### Commercial Stack (Current VELOS docs, per month)

| Service | Usage Level | Monthly Cost |
|---------|------------|-------------|
| Google 3D Tiles API | 100K requests/mo | ~$600 |
| Cesium Ion | Pro plan | $500 |
| Mapbox GL JS | 100K map loads | $500 |
| Google Maps API | 50K loads | $350 |
| CDN bandwidth | ~500 GB | $50 |
| **TOTAL** | | **~$2,000/month** |
| **Annual** | | **~$24,000/year** |

### Self-Hosted Stack (This Document)

| Item | Type | Monthly Cost |
|------|------|-------------|
| VPS (8 CPU, 32GB RAM, 500GB SSD) | One-time compute | $80-$150 |
| Domain + TLS (Let's Encrypt) | Free | $0 |
| OSM data download | Free | $0 |
| Sentinel-2 imagery | Free | $0 |
| SRTM/Copernicus DEM | Free | $0 |
| All software licenses | Open source | $0 |
| **TOTAL** | | **~$100-$150/month** |
| **Annual** | | **~$1,200-$1,800/year** |

### Savings: **~$22,000/year** (92% reduction)

### Additional Benefits

| Benefit | Commercial | Self-Hosted |
|---------|:----------:|:-----------:|
| Air-gap deployable | No | **Yes** |
| No rate limits | No | **Yes** |
| No usage tracking | No | **Yes** |
| Vendor lock-in risk | High | **None** |
| Data sovereignty | No (US servers) | **Full** |
| Works offline | No | **Yes** |
| Custom styling freedom | Limited | **Unlimited** |
| Response time | CDN varies | **Local (<1ms)** |

---

## 13. Implementation Guide {#13-implementation}

### Phase 1: Quick Start (Week 1 of E4's work)

```bash
# 1. Install prerequisites
sudo apt install tilemaker osmium-tool gdal-bin docker.io docker-compose

# 2. Download city data (example: Munich)
wget https://download.geofabrik.de/europe/germany/bayern/oberbayern-latest.osm.pbf
osmium extract -b 11.4,48.0,11.8,48.3 oberbayern-latest.osm.pbf -o munich.osm.pbf

# 3. Generate vector tiles
tilemaker --input munich.osm.pbf --output data/mbtiles/munich.mbtiles \
  --config resources/config-openmaptiles.json \
  --process resources/process-openmaptiles.lua

# 4. Generate 3D buildings from OSM
python3 scripts/osm_buildings_to_3dtiles.py --input munich.osm.pbf \
  --output data/3dtiles/munich/

# 5. Download terrain
python3 scripts/download_terrain.py --bbox 11.4 48.0 11.8 48.3 \
  --output-terrarium data/terrain/ --output-cesium data/terrain_cesium/

# 6. Start services
docker compose up -d

# 7. Open in browser
# MapLibre:  http://localhost:8081  (TileServer GL preview)
# CesiumJS:  http://localhost:8080/viewer.html  (custom viewer)
```

### Phase 2: Production Hardening (Month 5-6)

- Add Let's Encrypt TLS (Caddy or certbot)
- PMTiles for serverless tile hosting (S3-compatible object storage)
- CDN caching layer (Varnish or nginx proxy_cache) for multi-user access
- Tile pre-generation for all zoom levels (avoid runtime rendering)
- Monitoring: Prometheus + Grafana for tile serving metrics

### Phase 3: Add More Cities (Ongoing)

```bash
# Each new city deployment:
./add-city.sh ho-chi-minh "106.5,10.7,106.9,10.9"
./add-city.sh berlin "13.2,52.3,13.6,52.7" "ftp://download-berlin3d..."
./add-city.sh nyc "-74.1,40.6,-73.8,40.9"
```

---

## Appendix: Dependency Summary

### Complete Open-Source Stack

| Layer | Tool | License | Role |
|-------|------|---------|------|
| **Vector tiles** | tilemaker / Planetiler | BSD / Apache 2.0 | OSM → .mbtiles |
| **Tile server** | Martin | Apache 2.0 | Serve vector tiles |
| **3D buildings** | py3dtilers + citygml-tools | Apache 2.0 | CityGML → 3D Tiles |
| **3D from OSM** | osm2world (CLI only⚠️) + osmnx | GPL (tool) / MIT | OSM → 3D models |
| **Terrain** | GDAL + rio-rgbify + ctb-tile | MIT / BSD | DEM → terrain tiles |
| **Imagery** | Sentinel-2 + gdal2tiles | Public / MIT | Satellite → raster tiles |
| **Map renderer** | MapLibre GL JS | BSD-3 | Web map rendering |
| **3D globe** | CesiumJS (self-hosted) | Apache 2.0 | 3D geospatial |
| **Data layers** | deck.gl | MIT | GPU data viz |
| **Style editor** | Maputnik | MIT | Visual style editing |
| **Static server** | Nginx / Caddy | BSD / Apache 2.0 | Serve all static files |
| **Database** | PostGIS | GPL | Spatial queries |
| **Tile preview** | TileServer GL | BSD-2 | Raster rendering |
| **Spatial tools** | osmium-tool | GPL | OSM data manipulation |

**Total commercial API keys required: 0**
**Total SaaS subscriptions required: 0**
**Total license fees: $0**

### License Compatibility Notes

| Category | Tools | Safe for VELOS (Proprietary)? |
|----------|-------|:----------------------------:|
| **MIT / BSD / Apache 2.0** | Martin, MapLibre GL JS, CesiumJS, deck.gl, Planetiler, py3dtilers, Maputnik, PDAL, rio-rgbify, ctb-tile, 3d-tiles-tools, TileServer GL, osmnx | **Yes** — link, embed, modify freely |
| **GPL (CLI tool only)** | osm2world, osmium-tool, PostGIS | **Yes** — use as external tool in build pipeline. Do NOT link as library into VELOS binary |
| **CC-BY (data/schema)** | OpenMapTiles schema | **Yes** — requires attribution "© OpenMapTiles" on map |
| **ODbL (data)** | OpenStreetMap data | **Yes** — requires attribution "© OpenStreetMap contributors" |
| **Public Domain** | SRTM, Sentinel-2, Landsat, Copernicus DEM | **Yes** — no restrictions |

> **Rule of thumb:** All GPL tools (osm2world, osmium, PostGIS) are used as **external processes** in VELOS's data pipeline, not linked into the simulation engine. Their GPL license applies to the tool, not to the output data they produce. This is a safe and well-established pattern.

---

*v1.0 — Companion to VELOS 3D Visualization Architecture*
*Cross-references: §5 (CesiumJS), §6 (deck.gl), §10 (3D Tiles pipeline)*
