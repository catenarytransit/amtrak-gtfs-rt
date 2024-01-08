# amtrak-gtfs-rt
Decrypts Amtrak's GTFS-RT

A valid Amtrak GTFS structure must be passed into the function to work.

Here's an example of some working code!
Note that `prost` version `0.11` should be used, as `gtfs-rt` does not use `0.12` yet.
```rust 
extern crate amtrak_gtfs_rt;

use prost::Message;
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

## Capital Corridor Exception
Note that the Metropolitan Transportation Commission also publishes Capital Corridor in their own feed.
https://511.org/open-data/transit provides Capital Corridor as "CC". This data refreshes more often (and is closer in location & time), and shows locomotive numbers.
For this reason, you may wish to remove Capital Corridor from this feed.
Thus, we've included a function `filter_capital_corridor()` which takes in any `gtfs_rt::FeedMessage` and removes CC vehicles and trips.