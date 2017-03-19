#![feature(plugin)]
#![plugin(rocket_codegen)]

extern crate bzip2;
extern crate bincode;
extern crate rocket;
extern crate geogrid;
extern crate lru_cache;
extern crate serde_json;
extern crate stopwatch;
extern crate walkdir;
#[macro_use] extern crate rocket_contrib;
#[macro_use] extern crate serde_derive;

use std::hash::{Hash, Hasher};
use std::sync::Mutex;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::fs::File;

use bzip2::read::BzDecoder;
use geogrid::{GeoGrid, Bounds};
use geogrid::util::{match_shape, roads_from_json, node_bounds, mat_to_img};
use lru_cache::LruCache;
use rocket::{State, Request, Data, Outcome};
use rocket::http::uri::URI;
use rocket::http::Status;
use rocket::data::FromData;
use rocket::response::NamedFile;
use rocket::request::FromParam;
use rocket_contrib::{JSON, Value};
use stopwatch::Stopwatch;
use walkdir::WalkDir;

const GRID_DIM: usize = 9000;

#[derive(Debug, Clone)]
struct Location {
    name: String,
    filepath: PathBuf,
}

impl PartialEq for Location {
    fn eq(&self, other: &Self) -> bool {
        self.filepath == other.filepath
    }
}
impl Eq for Location {}

impl Hash for Location {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.filepath.hash(state);
    }
}

impl Location {
    fn new(name: &str) -> Option<Location> {
        // Find a file containing name and "geojson."
        for entry in WalkDir::new(".").into_iter()
                                      .filter_map(|e| e.ok())
                                      .filter(|e| e.file_type().is_file()) {
            let fname = entry.path().file_name().unwrap().to_str();
            if let Some(fname) = fname {
                if fname.contains(name) &&
                    (fname.ends_with("geojson") || fname.ends_with("geojson.bz2")) {
                    let filepath = PathBuf::from(entry.path());
                    return Some(Location {
                        name: String::from(name), filepath: filepath
                    });
                }
            }
        }
        None
    }

    fn predefined_bounds(&self) -> Option<Bounds> {
        // Look for some easy bounds predefined.
        for entry in WalkDir::new(".").into_iter()
                                      .filter_map(|e| e.ok())
                                      .filter(|e| e.file_type().is_file()) {
            let fname = entry.path().file_name().unwrap().to_str();
            if let Some(fname) = fname {
                if fname.contains(&self.name) && fname.contains("bounds") && fname.ends_with("json") {
                    let bounds = if let Some(reader) = file_reader(entry.path()) {
                        println!("Reading predefined bounds from {:?}", entry.path());
                        serde_json::from_reader(reader).ok()
                    } else {
                        println!("Could not make reader for {:?}", entry.path());
                        None
                    };
                    if bounds.is_some() {
                        return bounds;
                    }
                }
            }
        }
        None
    }

    fn find_bounds(&self) -> Bounds {
        self.predefined_bounds().unwrap_or_else(|| node_bounds(roads_from_json(
                    file_reader(&self.filepath).unwrap(), None).iter().flat_map(|r| r.iter())))
    }

    fn parent(&self) -> PathBuf {
        PathBuf::from(self.filepath.parent().unwrap_or(Path::new(".")))
    }
}

impl<'a> FromParam<'a> for Location {
    type Error = &'static str;
    fn from_param(param: &str) -> Result<Self, Self::Error> {
        let decoded = URI::percent_decode_lossy(param.as_bytes());
        let maybe_location = Location::new(&decoded.to_lowercase());
        if let Some(loc) = maybe_location {
            Ok(loc)
        } else {
            Err("Could not find a road json for requested location.")
        }
    }
}

fn file_reader<P: AsRef<Path>>(p: P) -> Option<Box<BufRead>> {
    if let Some(file) = File::open(&p).ok() {
        if p.as_ref().to_str().unwrap().ends_with(".bz2") {
            Some(Box::new(BufReader::new(BzDecoder::new(BufReader::new(file)))))
        } else {
            Some(Box::new(BufReader::new(file)))
        }
    } else {
        None
    }
}

#[derive(Clone)]
struct SavedLocation {
    geo: GeoGrid,
    dt: Vec<i32>
}

impl SavedLocation {
    fn new(location: &Location) -> Option<SavedLocation> {
        let reader = file_reader(&location.filepath);
        if let Some(reader) = reader {
            println!("Data found, processing for {:?}...", location);
            let mut s = Stopwatch::start_new();
            let nodes = roads_from_json(reader, location.predefined_bounds());
            println!("JSON data parse took {} ms, {} roads", s.elapsed_ms(), nodes.len());
            s.restart();
            let mut geo = GeoGrid::from_roads(&nodes, (GRID_DIM, GRID_DIM), true);
            println!("{:?} grid construction took {} ms", geo.size(), s.elapsed_ms());
            s.restart();
            let mut dt = geo.l1dist_transform();
            for v in dt.iter_mut() {
                *v = (*v)*(*v);
            }
            println!("Distance transform took {} ms", s.elapsed_ms());
            s.restart();
            // Save some space by clearing the grid since we only need the distance transform now.
            mat_to_img(geo.grid(), geo.size(),
                       location.parent().join(format!("{}.grid.png", location.name)), None);
            mat_to_img(&dt, geo.size(),
                       location.parent().join(format!("{}.dt.png", location.name)), Some((0, 150)));
            println!("Image saving took {} ms", s.elapsed_ms());
            geo.clear_grid();
            Some(SavedLocation{
                geo: geo,
                dt: dt
            })
        } else {
            None
        }
    }
}

#[derive(Debug, Deserialize)]
struct MatchData {
    shape: Vec<Vec<bool>>,
    bounds: Bounds
}

impl FromData for MatchData {
    type Error = &'static str;
    fn from_data(_: &Request, data: Data) -> Outcome<Self, (Status, &'static str), Data> {
        Outcome::Success(serde_json::from_reader(data.open()).unwrap())
    }
}

#[get("/")]
fn index() -> Option<NamedFile> {
    static_file(PathBuf::from("index.html"))
}

#[get("/static/<file..>")]
fn static_file(file: PathBuf) -> Option<NamedFile> {
    let root = match std::env::current_exe() {
        Ok(p) => PathBuf::from(p.parent().unwrap_or(Path::new("."))),
        Err(_) => PathBuf::from(".")
    };
    NamedFile::open(root.join(Path::new("static/")).join(&file)).ok().or(
    NamedFile::open(Path::new("static/").join(&file)).ok())
}

#[get("/bounds/<location>")]
fn bounds(saved_locs: State<Mutex<LruCache<Location, SavedLocation>>>, location: Location) -> JSON<Value> {
    let mut saved_locs = saved_locs.lock().unwrap();
    JSON(serde_json::to_value(saved_locs.get_mut(&location)
                              .map(|s| s.geo.bbox()).unwrap_or_else(|| location.find_bounds())).unwrap())
}

/// From a buffer of given original dimensions, extract a subsection starting from given index with
/// provided dimensions.
fn subrect<T:Copy>(buf: &[T], buf_cols: usize, start: usize, subdim: (usize, usize)) -> Vec<T> {
    let (s, t) = subdim;
    let mut v = Vec::with_capacity(s * t);
    let bufr = start / buf_cols;
    let bufc = start % buf_cols;
    for i in 0..s {
        for j in 0..t {
            v.push(buf[(bufr + i) * buf_cols + bufc + j]);
        }
    }
    v
}

#[post("/find_match/<location>", data="<data>")]
fn find_match(saved_locs: State<Mutex<LruCache<Location, SavedLocation>>>, location: Location, data: MatchData) -> Option<JSON<Value>> {
    // Within data.bounds, find data.shape.
    let b = data.bounds;
    let saved_geo;
    let subdt;
    let substart;
    let subdim;
    // Copy out the information we need to release the lock as soon as possible.
    {
        let mut saved_locs = saved_locs.lock().unwrap();
        if !saved_locs.contains_key(&location) {
            if let Some(loc) = SavedLocation::new(&location) {
                saved_locs.insert(location.clone(), loc);
            } else {
                return None;
            }
        }
        let saved = saved_locs.get_mut(&location).unwrap();
        saved_geo = saved.geo.clone();
        let subgrid = saved_geo.bounded_subgrid(b.north, b.south, b.east, b.west);
        substart = subgrid.0;
        subdim = subgrid.1;
        subdt = subrect(&saved.dt, saved_geo.size().1, substart, subdim);
    }
    let mut s = Stopwatch::start_new();
    let (r, w) = subdim;
    println!("Finding matches on {} x {} subgrid at {}", r, w, substart);
    // TODO: put this computation into a queue.
    let cm = match_shape(&subdt, (r, w), &data.shape);
    println!("Match finding took {} ms...", s.elapsed_ms());
    s.restart();
    mat_to_img(&cm, (r, w),
                location.parent().join(format!("{}.cm.png", location.name)), Some((0, 10000)));
    let mut cm: Vec<(usize, i32)> = cm.iter().enumerate().map(|(i, &v)| (i ,v)).collect();
    // TODO: use a parallel sort algorithm
    cm.sort_by_key(|&(_, v)| v);
    let topk: Vec<(f32, f32)> = cm.into_iter().map(|(i, s)| {
        // Translate subgrid coordinate back to full grid coordinate.
        let subrows = i / w;
        let subcolumns = i % w;
        let (s_lat, s_lon) = saved_geo.to_lat_lon(substart);
        let (r_lat, r_lon) = saved_geo.degree_resolution();
        println!("{} {} {} + ({} {}) from {} with {}", w, subrows, subcolumns, s_lat, s_lon, substart, s);
        (s_lat - subrows as f32 * r_lat, s_lon + subcolumns as f32 * r_lon)
    }).take(11).collect();
    println!("Match sorting and writing took {} ms", s.elapsed_ms());
    Some(JSON(json!({"best": topk, "scale": saved_geo.degree_resolution()})))
}

fn main() {
    let saved_locs: Mutex<LruCache<Location, SavedLocation>> = Mutex::new(LruCache::new(3));
    rocket::ignite().manage(saved_locs)
                    .mount("/", routes![index, static_file, bounds, find_match])
                    .launch();
}
