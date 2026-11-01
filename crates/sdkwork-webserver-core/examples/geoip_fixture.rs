//! Generate the deterministic MaxMind DB fixture used by the GeoIP evidence.
//!
//! Usage (from the repository root):
//!
//! ```text
//! cargo run -p sdkwork-webserver-core --example geoip_fixture -- tests/nginx/geoip
//! ```
//!
//! The output is three synthetic MMDB files:
//!
//! - `GeoIP2-Country-Test.mmdb`
//! - `GeoIP2-City-Test.mmdb`
//! - `GeoIP2-ASN-Test.mmdb`
//!
//! The data is entirely synthetic and uses the RFC 5737 documentation ranges
//! (`198.51.100.0/24`, `203.0.113.0/24`). No MaxMind/GeoLite data is embedded
//! or redistributed. The build epoch is fixed so repeated runs produce
//! byte-identical files.
//!
//! The writer is a dev-dependency (`mmdb-writer`, MIT OR Apache-2.0); this
//! example is never compiled into the release binary.

use std::{fs, path::PathBuf, time::SystemTime};

use mmdb_writer::{Ipv4Aliasing, Value, Writer};

/// Fixed MMDB build timestamp (the Unix epoch) for reproducibility.
const BUILD_EPOCH: SystemTime = SystemTime::UNIX_EPOCH;

fn names(english: &str) -> Value {
    Value::map([("en", Value::from(english))])
}

fn country(iso_code: &str, english: &str, continent: &str, continent_name: &str) -> Value {
    Value::map([
        (
            "continent",
            Value::map([
                ("code", Value::from(continent)),
                ("names", names(continent_name)),
            ]),
        ),
        (
            "country",
            Value::map([
                ("iso_code", Value::from(iso_code)),
                ("names", names(english)),
            ]),
        ),
    ])
}

fn city(
    record: Value,
    city: &str,
    latitude: f64,
    longitude: f64,
    metro: u16,
    postal: &str,
    region: &str,
    region_name: &str,
) -> Value {
    let Value::Map(mut entries) = record else {
        return record;
    };
    entries.insert("city".to_owned(), Value::map([("names", names(city))]));
    entries.insert(
        "location".to_owned(),
        Value::map([
            ("latitude", Value::from(latitude)),
            ("longitude", Value::from(longitude)),
            ("metro_code", Value::from(metro)),
        ]),
    );
    entries.insert(
        "postal".to_owned(),
        Value::map([("code", Value::from(postal))]),
    );
    entries.insert(
        "subdivisions".to_owned(),
        Value::array([Value::map([
            ("iso_code", Value::from(region)),
            ("names", names(region_name)),
        ])]),
    );
    Value::Map(entries)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "tests/nginx/geoip".to_owned()),
    );
    fs::create_dir_all(&output)?;

    let mut country_db = Writer::builder("GeoIP2-Country-Test")
        .build_epoch(BUILD_EPOCH)
        .ipv4_aliasing(Ipv4Aliasing::Disabled)
        .build();
    country_db.insert(
        "198.51.100.0/24".parse::<mmdb_writer::ipnet::IpNet>()?,
        &country("US", "United States", "NA", "North America"),
    )?;
    country_db.insert(
        "203.0.113.0/24".parse::<mmdb_writer::ipnet::IpNet>()?,
        &country("DE", "Germany", "EU", "Europe"),
    )?;
    // The stream GeoIP differential connects from loopback, so the fixture
    // records the loopback peer as the US test record.
    country_db.insert(
        "127.0.0.1/32".parse::<mmdb_writer::ipnet::IpNet>()?,
        &country("US", "United States", "NA", "North America"),
    )?;
    fs::write(
        output.join("GeoIP2-Country-Test.mmdb"),
        country_db.to_bytes()?,
    )?;

    let mut city_db = Writer::builder("GeoIP2-City-Test")
        .build_epoch(BUILD_EPOCH)
        .ipv4_aliasing(Ipv4Aliasing::Disabled)
        .build();
    city_db.insert(
        "198.51.100.0/24".parse::<mmdb_writer::ipnet::IpNet>()?,
        &city(
            country("US", "United States", "NA", "North America"),
            "Testville",
            34.0522,
            -118.2437,
            807,
            "90210",
            "CA",
            "California",
        ),
    )?;
    city_db.insert(
        "203.0.113.0/24".parse::<mmdb_writer::ipnet::IpNet>()?,
        &city(
            country("DE", "Germany", "EU", "Europe"),
            "Testburg",
            52.5200,
            13.4050,
            0,
            "10115",
            "BE",
            "Berlin",
        ),
    )?;
    city_db.insert(
        "127.0.0.1/32".parse::<mmdb_writer::ipnet::IpNet>()?,
        &city(
            country("US", "United States", "NA", "North America"),
            "Testville",
            34.0522,
            -118.2437,
            807,
            "90210",
            "CA",
            "California",
        ),
    )?;
    fs::write(output.join("GeoIP2-City-Test.mmdb"), city_db.to_bytes()?)?;

    let mut org_db = Writer::builder("GeoIP2-ASN-Test")
        .build_epoch(BUILD_EPOCH)
        .ipv4_aliasing(Ipv4Aliasing::Disabled)
        .build();
    org_db.insert(
        "198.51.100.0/24".parse::<mmdb_writer::ipnet::IpNet>()?,
        &Value::map([
            ("autonomous_system_number", Value::from(64_500_u32)),
            ("autonomous_system_organization", Value::from("Example Org")),
        ]),
    )?;
    org_db.insert(
        "203.0.113.0/24".parse::<mmdb_writer::ipnet::IpNet>()?,
        &Value::map([
            ("autonomous_system_number", Value::from(64_501_u32)),
            (
                "autonomous_system_organization",
                Value::from("Beispiel GmbH"),
            ),
        ]),
    )?;
    org_db.insert(
        "127.0.0.1/32".parse::<mmdb_writer::ipnet::IpNet>()?,
        &Value::map([
            ("autonomous_system_number", Value::from(64_500_u32)),
            ("autonomous_system_organization", Value::from("Example Org")),
        ]),
    )?;
    fs::write(output.join("GeoIP2-ASN-Test.mmdb"), org_db.to_bytes()?)?;

    eprintln!(
        "wrote GeoIP2 {{Country,City,ASN}}-Test.mmdb to {}",
        output.display()
    );
    Ok(())
}
