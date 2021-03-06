extern crate dazone;

extern crate bincode;
extern crate cbor;
extern crate clap;
extern crate csv;
extern crate flate2;
extern crate lz4;
extern crate num_cpus;
extern crate pbr;
extern crate rmp;
extern crate rmp_serialize;
extern crate rustc_serialize;
extern crate serde;
extern crate serde_json;
extern crate simple_parallel;
extern crate snappy_framed;

use std::{fs, path};

use std::fmt::Debug;

use std::sync::Mutex;

use rustc_serialize::{Encodable, Decodable};

use dazone::Dx16Result;
use dazone::data;
use dazone::data::cap::{Capitanable, Mode};
use dazone::data::pbuf::Protobufable;
use serde::Serialize;

use pbr::ProgressBar;

use clap::{Arg, App};

fn main() {
    let matches = App::new("pack")
                      .about("repack data to better formats")
                      .arg(Arg::with_name("SET")
                               .short("s")
                               .long("set")
                               .help("pick a set (tiny, 1node, 5nodes)")
                               .takes_value(true))
                      .arg(Arg::with_name("TABLE")
                               .index(1)
                               .required(true)
                               .help("table to process"))
                      .arg(Arg::with_name("FORMAT")
                               .index(2)
                               .required(true)
                               .help("cap-gz or rmp-gz"))
                      .arg(Arg::with_name("PROGRESSBAR")
                               .required(false)
                               .long("pb")
                               .help("activate progressbar"))
                      .get_matches();
    let set = matches.value_of("SET").unwrap_or("5nodes");
    let table = matches.value_of("TABLE").unwrap();
    let dst = matches.value_of("FORMAT").unwrap();
    let pb = matches.is_present("PROGRESSBAR");
    if dst == "text-deflate" {
        panic!("text-deflate can not be used as the output format!");
    }
    match &*table {
        "rankings" => loop_files::<data::pod::Ranking>(set, "rankings", &*dst, pb).unwrap(),
        "uservisits" => loop_files::<data::pod::UserVisits>(set, "uservisits", &*dst, pb).unwrap(),
        t => panic!("unknwon table {}", &*t),
    }
}

fn loop_files<T>(set: &str, table: &str, dst: &str, show_pb:bool) -> Dx16Result<()>
    where T: Decodable + Encodable + Capitanable + Debug + Protobufable + Serialize
{
    let source_root = dazone::files::data_dir_for("text-deflate", set, table);
    let target_root = dazone::files::data_dir_for(dst, set, table);
    let _ = fs::remove_dir_all(target_root.clone());
    try!(fs::create_dir_all(target_root.clone()));
    let jobs: Dx16Result<Vec<(path::PathBuf, path::PathBuf)>> =
        dazone::files::files_for_format(set, table, "text-deflate")
            .map(|entry| {
                let entry: String = entry.to_str().unwrap().to_string();
                let target = target_root.clone() +
                             &entry[source_root.len()..entry.find(".").unwrap()] +
                             "." + dst;
                Ok((path::PathBuf::from(&*entry), path::PathBuf::from(&target)))
            })
            .collect();
    let jobs = try!(jobs);

    let pb = Mutex::new(ProgressBar::new(jobs.len()));
    let mut pool = simple_parallel::Pool::new(2 * num_cpus::get());
    let task = |job: (path::PathBuf, path::PathBuf)| {
        let input = flate2::FlateReadExt::zlib_decode(fs::File::open(job.0.clone()).unwrap());
        let mut reader = csv::Reader::from_reader(input).has_headers(false);
        let tokens: Vec<&str> = dst.split("-").collect();
        let compressor = dazone::files::compressor::Compressor::for_format(dst);

        match tokens[0] {
            "bincode" => {
                let mut compressed = compressor.write_file(job.1);
                for item in reader.decode() {
                    let item: T = item.unwrap();
                    bincode::rustc_serialize::encode_into(&item,
                                                          &mut compressed,
                                                          bincode::SizeLimit::Infinite)
                        .unwrap();
                }
            }
            "buren" => {
                fs::create_dir_all(job.1.clone()).unwrap();
                let mut coder = ::dazone::buren::Serializer::new(job.1, compressor);
                for item in reader.decode() {
                    let item: T = item.unwrap();
                    item.serialize(&mut coder).unwrap();
                }
            }
            "cap" => {
                let mut compressed = compressor.write_file(job.1);
                for item in reader.decode() {
                    let item: T = item.unwrap();
                    item.write_to_cap(&mut compressed, Mode::Unpacked).unwrap();
                }
            }
            "cbor" => {
                let mut compressed = compressor.write_file(job.1);
                let mut coder = ::cbor::Encoder::from_writer(&mut compressed);
                for item in reader.decode() {
                    let item: T = item.unwrap();
                    item.encode(&mut coder).unwrap();
                }
            }
            "csv" => {
                let compressed = compressor.write_file(job.1);
                let mut coder = ::csv::Writer::from_writer(compressed);
                for item in reader.decode() {
                    let item: T = item.unwrap();
                    coder.encode(item).unwrap();
                }
            }
            "json" => {
                let mut compressed: Box<::std::io::Write> = compressor.write_file(job.1);
                for item in reader.decode() {
                    let item: T = item.unwrap();
                    ::serde_json::ser::to_writer(&mut compressed, &item).unwrap();
                    write!(compressed, "\n").unwrap();
                }
            }
            "mcap" => {
                let mut compressed = compressor.write_file(job.1);
                for item in reader.decode() {
                    let item: T = item.unwrap();
                    item.write_to_cap(&mut compressed, Mode::Mappable).unwrap();
                }
            }
            "pbuf" => {
                let mut compressed = compressor.write_file(job.1);
                for item in reader.decode() {
                    let item: T = item.unwrap();
                    item.write_to_pbuf(&mut compressed).unwrap();
                }
            }
            "pcap" => {
                let mut compressed = compressor.write_file(job.1);
                for item in reader.decode() {
                    let item: T = item.unwrap();
                    item.write_to_cap(&mut compressed, Mode::Packed).unwrap();
                }
            }
            "rmp" => {
                let mut compressed = compressor.write_file(job.1);
                let mut coder = ::rmp_serialize::Encoder::new(&mut compressed);
                for item in reader.decode() {
                    let item: T = item.unwrap();
                    item.encode(&mut coder).unwrap();
                }
            }
            any => panic!("unknown format {}", any),
        }
        if show_pb {
            pb.lock().unwrap().inc();
        }
    };
    pool.for_(jobs, &task);
    Ok(())
}
