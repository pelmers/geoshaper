#![feature(plugin)]
#![plugin(rocket_codegen)]

extern crate bzip2;
extern crate bincode;
extern crate quicksort;
extern crate rocket;
extern crate geogrid;
extern crate lru_cache;
extern crate serde_json;
extern crate stopwatch;
extern crate getopts;
#[macro_use]
extern crate rocket_contrib;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate lazy_static;
#[cfg(feature="opencl")]
extern crate ocl;

use std::process;
use std::sync::Mutex;
use std::path::{Path, PathBuf};
use std::env;

use geogrid::util::{Processor, match_shape, mat_to_img};

use getopts::Options;
use lru_cache::LruCache;
use rocket::State;
use rocket::response::NamedFile;
use rocket_contrib::{JSON, Value};
use stopwatch::Stopwatch;

mod types;
use types::*;

#[cfg(feature="opencl")]
mod opencl {
    pub use ocl::Device;
    pub fn device_for_index(idx: usize) -> Option<Device> {
        use ocl::builders::DeviceSpecifier;
        (DeviceSpecifier::All)
            .to_device_list(None)
            .ok()
            .and_then(|all_devices| if idx < all_devices.len() {
                let device = all_devices[idx];
                println!("Device {} selected", device.name());
                Some(device)
            } else {
                None
            })
    }
}

#[cfg(not(feature="opencl"))]
mod opencl {
    pub use geogrid::util::Device;
    pub fn device_for_index(_: usize) -> Option<Device> {
        println!("Warning: opencl feature not enabled, option ignored.");
        None
    }
}

use opencl::*;

/// Struct used to store parsed command line arguments or other configuration.
struct GlobalConfig {
    ocl_device: Option<Device>,
    sequential: bool,
    cache_size: usize,
    grid_dim: usize,
}

lazy_static! {
    static ref CONFIG: GlobalConfig = {
        let args: Vec<String> = env::args().collect();
        let program = args[0].clone();
        let mut opts = Options::new();
        opts.optopt("d", "device", "set a device index for OpenCL", "DEVICE");
        opts.optopt("w", "width", "set in pixels maximum dimension of grid", "WIDTH");
        opts.optopt("c", "cache", "number of recent locations to cache (default: 3)", "CACHE");
        opts.optflag("s", "sequential", "run shape matching algorithm in sequential mode");
        opts.optflag("h", "help", "print this help menu");
        let matches = match opts.parse(&args[1..]) {
            Ok(m) => { m }
            Err(f) => { panic!(f.to_string()) }
        };
        if matches.opt_present("h") {
            print!("{}", opts.usage(&program));
            process::exit(0);
        }
        GlobalConfig {
            ocl_device: matches.opt_str("d").and_then(|s| {
                s.parse::<usize>().ok().and_then(device_for_index)
            }),
            sequential: matches.opt_present("s"),
            cache_size: matches.opt_str("c").and_then(|s| s.parse::<usize>().ok()).unwrap_or(3),
            grid_dim: matches.opt_str("w").and_then(|s| s.parse::<usize>().ok()).unwrap_or(9000),
        }
    };
}

#[get("/")]
fn index() -> Option<NamedFile> {
    static_file(PathBuf::from("index.html"))
}

#[get("/static/<file..>")]
fn static_file(file: PathBuf) -> Option<NamedFile> {
    // Load static file from directory relative to either exe location or cwd.
    let root = match env::current_exe() {
        Ok(p) => PathBuf::from(p.parent().unwrap_or_else(|| Path::new("."))),
        Err(_) => PathBuf::from("."),
    };
    NamedFile::open(root.join(Path::new("static/")).join(&file))
        .ok()
        .or_else(|| NamedFile::open(Path::new("static/").join(&file)).ok())
}

#[get("/map_list")]
fn map_list() -> String {
    map_filename_match(|s| s.contains("geojson"))
        .iter()
        .filter_map(|p| p.file_name().map(|o| o.to_string_lossy()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[get("/bounds/<location>")]
fn bounds(saved_locs: State<Mutex<LruCache<Location, SavedLocation>>>,
          location: Location)
          -> JSON<Value> {
    let mut saved_locs = saved_locs.lock().unwrap();
    JSON(serde_json::to_value(saved_locs.get_mut(&location)
            .map(|s| s.geo.bbox())
            .unwrap_or_else(|| location.find_bounds()))
        .unwrap())
}

/// From a buffer of given original dimensions, extract a subsection starting from given index with
/// provided dimensions.
fn subrect<T: Copy>(buf: &[T], buf_cols: usize, start: usize, subdim: (usize, usize)) -> Vec<T> {
    let (s, t) = subdim;
    if s * t + start > buf.len() {
        return vec![];
    }
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
fn find_match(saved_locs: State<Mutex<LruCache<Location, SavedLocation>>>,
              location: Location,
              data: MatchData)
              -> Option<JSON<Value>> {
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
            if let Some(loc) = SavedLocation::new(&location, CONFIG.grid_dim) {
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
    let cm = if let Some(ref device) = CONFIG.ocl_device {
        println!("Using device {}", device.name());
        match_shape(&subdt, (r, w), &data.shape, 2, Processor::GPU(device, 256))
    } else if CONFIG.sequential {
        match_shape(&subdt, (r, w), &data.shape, 2, Processor::SingleCore)
    } else {
        match_shape(&subdt, (r, w), &data.shape, 2, Processor::MultiCore)
    };
    println!("Match finding took {} ms...", s.elapsed_ms());
    s.restart();
    mat_to_img(&cm,
               (r, w),
               location.parent().join(format!("{}.cm.png", location.name)),
               Some((0, 1000)));
    let mut cm: Vec<_> = cm.iter().enumerate().filter(|&(_, v)| *v >= 0).collect();
    // Use an in-place sort because the vector may be several hundred megabytes.
    quicksort::quicksort_by(&mut cm, |&a, &b| a.1.cmp(b.1));
    let topk: Vec<(f32, f32)> = cm.into_iter()
        .map(|(i, s)| {
            // Translate subgrid coordinate back to full grid coordinate.
            let subrows = i / w;
            let subcolumns = i % w;
            let (s_lat, s_lon) = saved_geo.to_lat_lon(substart);
            let (r_lat, r_lon) = saved_geo.degree_resolution();
            println!("{} {} {} + ({} {}) from {} with {}",
                     w,
                     subrows,
                     subcolumns,
                     s_lat,
                     s_lon,
                     substart,
                     s);
            (s_lat - subrows as f32 * r_lat, s_lon + subcolumns as f32 * r_lon)
        })
        .take(11)
        .collect();
    println!("Match sorting and writing took {} ms", s.elapsed_ms());
    Some(JSON(json!({"best": topk, "scale": saved_geo.degree_resolution()})))
}

fn main() {
    lazy_static::initialize(&CONFIG);
    let saved_locs: Mutex<LruCache<Location, SavedLocation>> =
        Mutex::new(LruCache::new(CONFIG.cache_size));
    rocket::ignite()
        .manage(saved_locs)
        .mount("/",
               routes![index, static_file, map_list, bounds, find_match])
        .launch();
}
