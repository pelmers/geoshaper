use serde_json;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::fs::{File, read_dir};
use std::env;
use std::iter;
use std::borrow::Cow;

use bzip2::read::BzDecoder;
use geogrid::{GeoGrid, Bounds};
use geogrid::util::{roads_from_json, node_bounds, mat_to_img};
use rocket::{Request, Data, Outcome};
use rocket::http::uri::URI;
use rocket::http::Status;
use rocket::data::FromData;
use rocket::request::FromParam;
use stopwatch::Stopwatch;


#[derive(Debug, Clone)]
pub struct Location {
    pub name: String,
    pub filepath: PathBuf,
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

pub fn map_filename_match<P: Fn(Cow<str>) -> bool>(pred: P) -> Vec<PathBuf> {
    let mut results = vec![];
    // Look either in the current folder or in the folder of the executable.
    for root in env::current_exe()
        .iter()
        .chain(iter::once(&PathBuf::from(".")))
        .filter_map(|p| p.parent()) {
        // Join root with "maps" and read_dir for the needed file.
        if let Ok(paths) = read_dir(root.join(Path::new("maps/"))) {
            for entry in paths.filter_map(|p| p.ok()) {
                if pred(entry.file_name().to_string_lossy()) {
                    results.push(entry.path());
                }
            }
        }
    }
    results
}

impl Location {
    pub fn new(name: &str) -> Option<Location> {
        map_filename_match(|fname| {
                fname.contains(name) &&
                (fname.ends_with("geojson") || fname.ends_with("geojson.bz2"))
            })
            .into_iter()
            .next()
            .map(|p| {
                Location {
                    name: String::from(name),
                    filepath: p,
                }
            })
    }

    pub fn predefined_bounds(&self) -> Option<Bounds> {
        // Look for some easy bounds predefined.
        map_filename_match(|fname| {
                fname.contains(&self.name) && fname.contains("bounds") && fname.ends_with("json")
            })
            .into_iter()
            .next()
            .and_then(|p| file_reader(p).and_then(|reader| serde_json::from_reader(reader).ok()))
    }

    pub fn find_bounds(&self) -> Bounds {
        self.predefined_bounds().unwrap_or_else(|| {
            node_bounds(roads_from_json(file_reader(&self.filepath).unwrap(), None)
                .iter()
                .flat_map(|r| r.iter()))
        })
    }

    pub fn parent(&self) -> PathBuf {
        PathBuf::from(self.filepath.parent().unwrap_or_else(|| Path::new(".")))
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


#[derive(Clone)]
pub struct SavedLocation {
    pub geo: GeoGrid,
    pub dt: Vec<i32>,
}


fn file_reader<P: AsRef<Path>>(p: P) -> Option<Box<BufRead>> {
    if let Ok(file) = File::open(&p) {
        if p.as_ref().to_str().unwrap().ends_with(".bz2") {
            Some(Box::new(BufReader::new(BzDecoder::new(BufReader::new(file)))))
        } else {
            Some(Box::new(BufReader::new(file)))
        }
    } else {
        None
    }
}


impl SavedLocation {
    pub fn new(location: &Location, grid_dim: usize) -> Option<SavedLocation> {
        let reader = file_reader(&location.filepath);
        if let Some(reader) = reader {
            println!("Data found, processing for {:?}...", location);
            let mut s = Stopwatch::start_new();
            let nodes = roads_from_json(reader, location.predefined_bounds());
            println!("JSON data parse took {} ms, {} roads",
                     s.elapsed_ms(),
                     nodes.len());
            s.restart();
            let mut geo = GeoGrid::from_roads(&nodes, (grid_dim, grid_dim), true);
            println!("{:?} grid construction took {} ms",
                     geo.size(),
                     s.elapsed_ms());
            s.restart();
            let mut dt = geo.l1dist_transform();
            for v in &mut dt {
                *v *= *v;
            }
            println!("Distance transform took {} ms", s.elapsed_ms());
            s.restart();
            mat_to_img(geo.grid(),
                       geo.size(),
                       location.parent().join(format!("{}.grid.png", location.name)),
                       None);
            mat_to_img(&dt,
                       geo.size(),
                       location.parent().join(format!("{}.dt.png", location.name)),
                       Some((0, 150)));
            println!("Image saving took {} ms", s.elapsed_ms());
            // Save some space by clearing the grid since we only need the distance transform now.
            geo.clear_grid();
            Some(SavedLocation { geo: geo, dt: dt })
        } else {
            None
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct MatchData {
    pub shape: Vec<Vec<bool>>,
    pub bounds: Bounds,
}

impl FromData for MatchData {
    type Error = &'static str;
    fn from_data(_: &Request, data: Data) -> Outcome<Self, (Status, &'static str), Data> {
        Outcome::Success(serde_json::from_reader(data.open()).unwrap())
    }
}
