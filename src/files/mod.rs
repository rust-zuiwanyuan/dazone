use std::marker::PhantomData;
use snappy_framed::read::{CrcMode, SnappyFramedDecoder};
use flate2::FlateReadExt;

use rustc_serialize::Decodable;

use capnp::serialize::OwnedSegments;
use capnp::message::Reader;

use protobuf::MessageStatic;

use std::{fs, io, path};
use std::io::BufReader;

use crunch::BI;

pub mod bincode;
pub mod cap;
pub mod rmp;
pub mod csv;
pub mod cbor;
pub mod pbuf;

pub fn data_dir_for(state: &str, set: &str, table: &str) -> String {
    format!("data/{}/{}/{}", state, set, table)
}

pub fn files_for_format(set: &str, table: &str, format: &str) -> Vec<path::PathBuf> {
    let source_root = data_dir_for(format, set, table);
    let ext = if format == "text-deflate" {
        "deflate"
    } else {
        format
    };
    let glob = source_root.clone() + "/*." + ext;
    let mut vec: Vec<path::PathBuf> = ::glob::glob(&glob)
        .unwrap()
        .map(|p| p.unwrap().to_owned())
        .collect();
    vec.sort();
    vec
}

pub fn uncompressed_files_for_format<'a>(set: &str,
                                         table: &str,
                                         format: &str)
-> BI<'a, Box<io::Read + Send>> {
    let tokens: Vec<String> = format.split("-").map(|x| x.to_owned()).collect();
    Box::new(files_for_format(set, table, format).into_iter().map(move |f| {
        let file = fs::File::open(f).unwrap();

        let decompressed: Box<io::Read + Send> = if tokens.len() == 1 {
            Box::new(BufReader::new(file))
        } else if tokens[1] == "gz" {
            Box::new(file.gz_decode().unwrap())
        } else if tokens[1] == "deflate" {
            Box::new(file.zlib_decode())
        } else if tokens[1] == "snz" {
            Box::new(SnappyFramedDecoder::new(file, CrcMode::Ignore))
        } else {
            panic!("unknown compression {}", tokens[1]);
        };

        decompressed
    }))
}

pub fn bibi_pod<'a, 'b, T>(set: &str, table: &str, format: &str) -> BI<'a, BI<'b, T>>
where T: Decodable + Send + 'static
{
    let tokens: Vec<String> = format.split("-").map(|x| x.to_owned()).collect();
    Box::new(uncompressed_files_for_format(set, table, format).into_iter().map(move |f| {
        let it: BI<T> = match &*tokens[0] {
            "bincode" => Box::new(bincode::BincodeReader::new(f)),
            "cbor" => Box::new(cbor::CborReader::new(f)),
            "csv" | "text" => Box::new(csv::CSVReader::new(f)),
            "rmp" => Box::new(rmp::RMPReader::new(f)),
            any => panic!("unknown format {}", any),
        };
        it
    }))
}

pub fn bibi_cap<'a, 'b>(set: &str,
                        table: &str,
                        format: &str)
-> BI<'a, BI<'b, Reader<OwnedSegments>>> {
    Box::new(uncompressed_files_for_format(set, table, format)
             .map(|f| -> BI<Reader<OwnedSegments>> { Box::new(cap::CapReader::new(f)) }))
}

pub fn bibi_pbuf<'a, 'b, T>(set: &str,
                                          table: &str,
                                          format: &str)
-> BI<'a, BI<'b, T>> where T:MessageStatic+ Send{
    Box::new(uncompressed_files_for_format(set, table, format)
             .map(|f| -> BI<T> { Box::new(pbuf::PBufReader{ stream: f, phantom:PhantomData}) }))
}
