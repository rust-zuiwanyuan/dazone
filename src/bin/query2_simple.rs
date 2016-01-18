extern crate dazone;

use dazone::Dx16Result;
use dazone::crunch::*;
use dazone::short_bytes_array::*;
use dazone::data::pod::UserVisits;

fn main() {
    let input = dazone::files::csv::bibi::<UserVisits>("5nodes", "uservisits");
    let map = |record: Dx16Result<UserVisits>| {
        let visit = record.unwrap();
        Emit::One(K8::prefix(&*visit.source_ip), visit.ad_revenue)
    };
    let reduce = |a: &f32, b: &f32| a + b;
    let mut aggregator = ::dazone::crunch::aggregators::MultiHashMapAggregator::new(&reduce, 256);
    MapOp::new_map_reduce(map)
        .with_progress(true)
        .with_workers(16)
        .run(input, &mut aggregator);
    aggregator.converge();
    println!("### {} ###", aggregator.len());
}
