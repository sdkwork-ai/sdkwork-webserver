//! nginx `ngx_http_geoip_module` integration tests: nginx materialization,
//! typed TOML materialization, path confinement, and fail-closed diagnostics.
//!
//! The MMDB fixtures are generated in a temporary directory with the
//! `mmdb-writer` dev-dependency; no MaxMind/GeoLite data is used.

use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use mmdb_writer::{Ipv4Aliasing, Value, Writer};
use sdkwork_webserver_core::config::{load_server_toml_file, WebServerAppConfig};
use sdkwork_webserver_core::nginx::{
    materialize_nginx_app_default, parse_nginx_config, NginxConfigError,
};

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

/// Write the three synthetic databases and return their paths.
fn write_databases(dir: &Path) -> (String, String, String) {
    let mut country_db = Writer::builder("GeoIP2-Country-Test")
        .build_epoch(BUILD_EPOCH)
        .ipv4_aliasing(Ipv4Aliasing::Disabled)
        .build();
    country_db
        .insert(
            "198.51.100.0/24"
                .parse::<mmdb_writer::ipnet::IpNet>()
                .unwrap(),
            &country("US", "United States", "NA", "North America"),
        )
        .unwrap();
    country_db
        .insert(
            "203.0.113.0/24"
                .parse::<mmdb_writer::ipnet::IpNet>()
                .unwrap(),
            &country("DE", "Germany", "EU", "Europe"),
        )
        .unwrap();
    country_db
        .insert(
            "127.0.0.1/32".parse::<mmdb_writer::ipnet::IpNet>().unwrap(),
            &country("US", "United States", "NA", "North America"),
        )
        .unwrap();
    let country_path = dir.join("GeoIP2-Country-Test.mmdb");
    fs::write(&country_path, country_db.to_bytes().unwrap()).unwrap();

    let mut city_db = Writer::builder("GeoIP2-City-Test")
        .build_epoch(BUILD_EPOCH)
        .ipv4_aliasing(Ipv4Aliasing::Disabled)
        .build();
    let mut record = country("US", "United States", "NA", "North America");
    if let Value::Map(entries) = &mut record {
        entries.insert(
            "city".to_owned(),
            Value::map([("names", names("Testville"))]),
        );
        entries.insert(
            "location".to_owned(),
            Value::map([
                ("latitude", Value::from(34.0522_f64)),
                ("longitude", Value::from(-118.2437_f64)),
                ("metro_code", Value::from(807_u16)),
            ]),
        );
        entries.insert(
            "postal".to_owned(),
            Value::map([("code", Value::from("90210"))]),
        );
        entries.insert(
            "subdivisions".to_owned(),
            Value::array([Value::map([
                ("iso_code", Value::from("CA")),
                ("names", names("California")),
            ])]),
        );
    }
    city_db
        .insert(
            "198.51.100.0/24"
                .parse::<mmdb_writer::ipnet::IpNet>()
                .unwrap(),
            &record,
        )
        .unwrap();
    city_db
        .insert(
            "127.0.0.1/32".parse::<mmdb_writer::ipnet::IpNet>().unwrap(),
            &record,
        )
        .unwrap();
    let city_path = dir.join("GeoIP2-City-Test.mmdb");
    fs::write(&city_path, city_db.to_bytes().unwrap()).unwrap();

    let mut org_db = Writer::builder("GeoIP2-ASN-Test")
        .build_epoch(BUILD_EPOCH)
        .ipv4_aliasing(Ipv4Aliasing::Disabled)
        .build();
    org_db
        .insert(
            "198.51.100.0/24"
                .parse::<mmdb_writer::ipnet::IpNet>()
                .unwrap(),
            &Value::map([
                ("autonomous_system_number", Value::from(64_500_u32)),
                ("autonomous_system_organization", Value::from("Example Org")),
            ]),
        )
        .unwrap();
    org_db
        .insert(
            "127.0.0.1/32".parse::<mmdb_writer::ipnet::IpNet>().unwrap(),
            &Value::map([
                ("autonomous_system_number", Value::from(64_500_u32)),
                ("autonomous_system_organization", Value::from("Example Org")),
            ]),
        )
        .unwrap();
    let org_path = dir.join("GeoIP2-ASN-Test.mmdb");
    fs::write(&org_path, org_db.to_bytes().unwrap()).unwrap();

    (
        country_path.to_string_lossy().into_owned(),
        city_path.to_string_lossy().into_owned(),
        org_path.to_string_lossy().into_owned(),
    )
}

fn temp_databases() -> (tempfile::TempDir, String, String, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let (country, city, org) = write_databases(dir.path());
    (dir, country, city, org)
}

fn materialize(text: &str) -> Result<WebServerAppConfig, NginxConfigError> {
    let parsed = parse_nginx_config(text, Path::new("site.conf")).expect("parse");
    materialize_nginx_app_default(&parsed, Path::new("/etc/nginx/sites-enabled"), "geoip")
}

fn materialize_err(text: &str, expected: &str) {
    let error = materialize(text).expect_err("must fail closed");
    let message = error.to_string();
    assert!(
        message.contains(expected),
        "expected diagnostic containing `{expected}`, got: {message}"
    );
}

fn server_body(extra_http: &str) -> String {
    format!(
        r#"
{extra_http}
server {{
    listen 127.0.0.1:18081;
    server_name geoip.example.com;
    location / {{
        return 200 "ok";
    }}
}}
"#
    )
}

#[test]
fn materializes_all_five_nginx_geoip_directives() {
    let (_dir, country, city, org) = temp_databases();
    let text = server_body(&format!(
        r#"
geoip_country {country} utf8;
geoip_city {city};
geoip_org {org};
geoip_proxy 127.0.0.1;
geoip_proxy 10.0.0.0/8;
geoip_proxy_recursive on;
"#
    ));
    let app = materialize(&text).expect("materialize");
    let geoip = app.geoip.as_ref().expect("geoip config");
    assert_eq!(geoip.country.as_deref(), Some(country.as_str()));
    assert_eq!(geoip.city.as_deref(), Some(city.as_str()));
    assert_eq!(geoip.org.as_deref(), Some(org.as_str()));
    assert_eq!(geoip.proxies, vec!["127.0.0.1", "10.0.0.0/8"]);
    assert!(geoip.proxy_recursive);
}

#[test]
fn rejects_relative_duplicate_and_wrong_context_form() {
    let (_dir, country, _city, _org) = temp_databases();
    materialize_err(
        &server_body("geoip_country relative/GeoIP2-Country.mmdb;"),
        "must be absolute",
    );
    materialize_err(
        &server_body(&format!(
            "geoip_country {country};\ngeoip_country {country};"
        )),
        "is duplicate",
    );
    materialize_err(
        &format!(
            r#"
server {{
    listen 127.0.0.1:18081;
    server_name geoip.example.com;
    geoip_country {country};
    location / {{ return 200 "ok"; }}
}}
"#
        ),
        "only allowed in the `http` block",
    );
    materialize_err(
        &server_body(&format!("geoip_country {country} latin1;")),
        "invalid parameter",
    );
    materialize_err(&server_body("geoip_proxy not-a-network;"), "geoip_proxy");
}

#[test]
fn rejects_missing_invalid_and_wrong_edition_databases() {
    let (_dir, _country, city, _org) = temp_databases();
    materialize_err(
        &server_body("geoip_country /nonexistent/GeoIP2-Country.mmdb;"),
        "cannot be read",
    );
    // A City database is not a valid `geoip_country` edition.
    materialize_err(
        &server_body(&format!("geoip_country {city};")),
        "database_type",
    );
}

#[test]
fn materializes_all_three_stream_geoip_directives() {
    let (_dir, country, city, org) = temp_databases();
    let text = format!(
        r#"
stream {{
    geoip_country {country};
    geoip_city {city};
    geoip_org {org};
    server {{
        listen 127.0.0.1:18082;
        proxy_pass 127.0.0.1:19000;
    }}
}}
"#
    );
    let app = materialize(&text).expect("stream materialize");
    let geoip = app.stream_geoip.as_ref().expect("stream geoip config");
    assert_eq!(geoip.country.as_deref(), Some(country.as_str()));
    assert_eq!(geoip.city.as_deref(), Some(city.as_str()));
    assert_eq!(geoip.org.as_deref(), Some(org.as_str()));
    assert!(geoip.proxies.is_empty());
    assert!(!geoip.proxy_recursive);
}

#[test]
fn rejects_stream_geoip_proxy_trust_directives() {
    let (_dir, country, _city, _org) = temp_databases();
    let text = format!(
        r#"
stream {{
    geoip_country {country};
    geoip_proxy 127.0.0.1;
    server {{
        listen 127.0.0.1:18082;
        proxy_pass 127.0.0.1:19000;
    }}
}}
"#
    );
    materialize_err(&text, "HTTP GeoIP directive");
}

#[test]
fn typed_toml_materializes_geoip_and_confines_it_to_the_root() {
    let (dir, country, city, org) = temp_databases();
    let root = dir.path().to_string_lossy().replace('\\', "/");
    let toml = format!(
        r#"
[http]
geoipDatabaseRoot = "{root}"
[http.geoip]
country = "{country}"
city = "{city}"
org = "{org}"
proxies = ["127.0.0.1", "10.0.0.0/8"]
proxyRecursive = true

[[http.server]]
listen = ["127.0.0.1:18081"]
serverName = ["geoip.example.com"]

[[http.server.location]]
match = "/"
returnStatus = 200
returnBody = "ok"
"#
    );
    let path = dir.path().join("server.toml");
    fs::write(&path, &toml).expect("write toml");
    let app = load_server_toml_file(&path, "geoip").expect("typed materialize");
    let geoip = app.geoip.as_ref().expect("geoip config");
    assert_eq!(geoip.country.as_deref(), Some(country.as_str()));
    assert_eq!(geoip.proxies, vec!["127.0.0.1", "10.0.0.0/8"]);
    assert!(geoip.proxy_recursive);
    assert_eq!(
        app.nginx.geoip_database_root.as_deref(),
        Some(root.as_str())
    );

    // A database outside the operator root fails closed.
    let outside = toml.replace(
        &format!("country = \"{country}\""),
        "country = \"/tmp/outside.mmdb\"",
    );
    fs::write(&path, outside).expect("write toml");
    let error = load_server_toml_file(&path, "geoip").expect_err("must fail closed");
    let message = match &error {
        sdkwork_webserver_core::config::WebServerConfigError::Validation { diagnostics } => {
            diagnostics
                .iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.path, diagnostic.message))
                .collect::<Vec<_>>()
                .join("; ")
        }
        error => error.to_string(),
    };
    assert!(
        message.contains("geoipDatabaseRoot") || message.contains("outside"),
        "unexpected diagnostic: {message}"
    );
}

#[test]
fn typed_toml_materializes_stream_geoip() {
    let (dir, country, city, org) = temp_databases();
    let toml = format!(
        r#"
[http]
[[http.server]]
listen = ["127.0.0.1:18081"]
serverName = ["geoip.example.com"]
[[http.server.location]]
match = "/"
returnStatus = 200

[stream.geoip]
country = "{country}"
city = "{city}"
org = "{org}"

[[stream.server]]
listen = ["127.0.0.1:18082"]
proxyPass = "127.0.0.1:19000"
"#
    );
    let path = dir.path().join("server.toml");
    fs::write(&path, toml).expect("write toml");
    let app = load_server_toml_file(&path, "geoip").expect("typed stream materialize");
    let geoip = app.stream_geoip.as_ref().expect("stream geoip config");
    assert_eq!(geoip.country.as_deref(), Some(country.as_str()));
    assert!(geoip.proxies.is_empty());
}

#[test]
fn typed_toml_rejects_unknown_geoip_keys() {
    let (_dir, country, _city, _org) = temp_databases();
    let dir = tempfile::tempdir().expect("tempdir");
    let toml = format!(
        r#"
[http]
[http.geoip]
country = "{country}"
unexpected = true

[[http.server]]
listen = ["127.0.0.1:18081"]
serverName = ["geoip.example.com"]

[[http.server.location]]
match = "/"
returnStatus = 200
"#
    );
    let path = dir.path().join("server.toml");
    fs::write(&path, toml).expect("write toml");
    let error = load_server_toml_file(&path, "geoip").expect_err("must fail closed");
    assert!(error.to_string().contains("unexpected"), "got: {error}");
}

#[test]
fn geoip_database_fixture_files_are_readable() {
    // The committed evidence fixture must stay valid MMDB; the differential
    // runner consumes it directly.
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("nginx")
        .join("geoip");
    for name in [
        "GeoIP2-Country-Test.mmdb",
        "GeoIP2-City-Test.mmdb",
        "GeoIP2-ASN-Test.mmdb",
    ] {
        let path = dir.join(name);
        assert!(path.is_file(), "missing committed fixture {path:?}");
        let reader = maxminddb::Reader::open_readfile(&path).expect("valid MMDB");
        assert!(reader.metadata().database_type.starts_with("GeoIP2-"));
    }
}
