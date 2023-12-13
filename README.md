# amtrak-gtfs-rt
Decrypts Amtrak's GTFS-RT

A valid Amtrak GTFS structure must be passed into the function to work.

Here's an example of some working code!
```rust 
extern crate amtrak_gtfs_rt;

use prost::Message;
use redis::Client as RedisClient;

use kactus::insert::insert_gtfs_rt;
use kactus::insert::insert_gtfs_rt_bytes;

use kactus::aspen::send_to_aspen;

use gtfs_structures::Gtfs;

#[tokio::main]
async fn main() {
    let gtfs = Gtfs::from_url_async("https://content.amtrak.com/content/gtfs/GTFS.zip")
    .await
    .unwrap();

    let client = reqwest::Client::new();
    loop {
        let amtrak_gtfs_rt = amtrak_gtfs_rt::fetch_amtrak_gtfs_rt(&gtfs, &client).await.unwrap();

        //extract the binary data
        let vehicle_data = amtrak_gtfs_rt.vehicle_positions.encode_to_vec();
        let trip_data = amtrak_gtfs_rt.trip_updates.encode_to_vec();

        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}
```

This software package decrypts the Amtrak track-a-train json data and performs lookups of trip information in the GTFS schedule to match each vehicle with it's route_id and trip_id.

Pull requests are welcome!