## Geoshaper draws shapes in a map.

# Usage

Download the appropriate binaries from the release page.

Compiling it yourself requires rust nightly. After checkout, run with:
```
cargo run --release
```
Remark that because the map grids can be quite large, running in debug mode
will be very very slow.

By default runshapr will open a web server on port 8000. Change the port by
specifying the `ROCKET_PORT` environment variable.
For example to run on port 11111,
```
ROCKET_PORT=11111 cargo run --release
```

When the server receives a request for some location, it will look for a
corresponding OpenStreetMap GEOJSON file that contains its road data. I suggest
downloading from [MapZen](https://mapzen.com/data/metro-extracts/).

The GEOJSON file must be saved in a subdirectory of the working directory (for
example in ./maps) and it must have a name that contains the location and ends
in "geojson" or "geojson.bz2". For example, you could save the data file for
New York as "maps/new york.geojson". The name is not case sensitive but it is
space-sensitive.

Optionally you may also specify bounds for the location. Sometimes the map data
includes excessive suburbs. See
[houston_texas_bounds.json](maps/houston_texas_bounds.json) for an example. The
filename must contain the location name and the word "bounds" and end in
"json". Otherwise I use the entire input dataset.

Verify correctness by inspecting <location>.grid.png which should contain a
road grid of the location during the first shape trace.
