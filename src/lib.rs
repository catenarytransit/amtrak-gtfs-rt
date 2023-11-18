use openssl::aes::{AesKey, aes_ige};
use openssl::symm::Mode;
use openssl::encrypt::Decrypter;
use openssl::symm::decrypt;
use openssl::symm::Cipher;
extern crate crypto;
use base64::{Engine as _, alphabet, engine::{self, general_purpose}};
const CUSTOM_ENGINE: engine::GeneralPurpose =
    engine::GeneralPurpose::new(&alphabet::URL_SAFE, general_purpose::PAD);

const s_value: &str = "9a3686ac";
const i_value: &str = "c6eb2f7f5c4740c1a2f708fefd947d39";
const public_key: &str = "69af143c-e8cf-47f8-bf09-fc1f61e5cc33";
const master_segment: i32 = 86;

use std::{collections::HashMap, fmt};

use chrono::{DateTime, FixedOffset};
use serde::{de, Deserialize};

pub fn decrypt_data(content: &[u8], key: &[u8]) {
    let mut output_key: &mut [u8; 64];

    use base64::Engine;
    let mut buffer = Vec::<u8>::new();
    // with the default engine
    general_purpose::STANDARD
        .decode_vec(s_value, &mut buffer,).unwrap();
    println!("{:?}", buffer);

    let locked_salt = buffer.as_slice();

    buffer.clear();

    let _ = openssl::pkcs5::pbkdf2_hmac(
        locked_salt,
        1000 as usize,
        openssl::hash::MessageDigest::sha256(),
        output_key,
        ).unwrap();
}

pub async fn fetch_json_data(client: &reqwest::Client) {
    

    let amtrak_raw_data_encrypted = client.get("https://maps.amtrak.com/services/MapDataService/trains/getTrainsData").send().await;

    match amtrak_raw_data_encrypted {
        Ok(amtrak_raw_data_encrypted) => {
            let text = amtrak_raw_data_encrypted.text().await.unwrap();

            let content_hash_length = text.len() - master_segment;


        },
        Err(err) => {
            err
        }
    }
}

pub async fn fetch_amtrak_gtfs_rt(client: &reqwest::Client) {
        let json_data = fetch_json_data(&client);

        match json_data {
            Ok(json_data) => {

            },
            Err(err) => {
                err
            }
        }
}

#[cfg(test)]
mod tests {
    use super::*;
}
