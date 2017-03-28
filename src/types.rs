use serde_json;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::fs::File;

use bzip2::read::BzDecoder;
use geogrid::{GeoGrid, Bounds};
use geogrid::util::{roads_from_json, node_bounds, mat_to_img};
use rocket::{Request, Data, Outcome};
use rocket::http::uri::URI;
use rocket::http::Status;
use rocket::data::FromData;
use rocket::request::FromParam;
use stopwatch::Stopwatch;
use walkdir::WalkDir;


const GRID_DIM: usize = 9000;

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

impl Location {
    pub fn new(name: &str) -> Option<Location> {
        // Find a file containing name and "geojson."
        for entry in WalkDir::new(".")
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file()) {
            let fname = entry.path().file_name().unwrap().to_str();
            if let Some(fname) = fname {
                if fname.contains(name) &&
                   (fname.ends_with("geojson") || fname.ends_with("geojson.bz2")) {
                    let filepath = PathBuf::from(entry.path());
                    return Some(Location {
                        name: String::from(name),
                        filepath: filepath,
                    });
                }
            }
        }
        None
    }

    pub fn predefined_bounds(&self) -> Option<Bounds> {
        // Look for some easy bounds predefined.
        for entry in WalkDir::new(".")
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file()) {
            let fname = entry.path().file_name().unwrap().to_str();
            if let Some(fname) = fname {
                if fname.contains(&self.name) && fname.contains("bounds") &&
                   fname.ends_with("json") {
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
    pub fn new(location: &Location) -> Option<SavedLocation> {
        let reader = file_reader(&location.filepath);
        if let Some(reader) = reader {
            println!("Data found, processing for {:?}...", location);
            let mut s = Stopwatch::start_new();
            let nodes = roads_from_json(reader, location.predefined_bounds());
            println!("JSON data parse took {} ms, {} roads",
                     s.elapsed_ms(),
                     nodes.len());
            s.restart();
            let mut geo = GeoGrid::from_roads(&nodes, (GRID_DIM, GRID_DIM), true);
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
            // Save some space by clearing the grid since we only need the distance transform now.
            mat_to_img(geo.grid(),
                       geo.size(),
                       location.parent().join(format!("{}.grid.png", location.name)),
                       None);
            mat_to_img(&dt,
                       geo.size(),
                       location.parent().join(format!("{}.dt.png", location.name)),
                       Some((0, 150)));
            println!("Image saving took {} ms", s.elapsed_ms());
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
