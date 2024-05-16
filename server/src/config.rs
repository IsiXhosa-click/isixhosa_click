use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;
use warp::host::Authority;
use warp::http::{uri, Uri};

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub database_path: PathBuf,
    pub tantivy_path: PathBuf,
    pub cert_path: Option<PathBuf>,
    pub key_path: Option<PathBuf>,
    pub static_site_files: PathBuf,
    pub other_static_files: PathBuf,
    pub log_path: PathBuf,
    pub http_port: u16,
    pub https_port: u16,
    pub host: String,
    pub oidc_client: String,
    pub oidc_secret: String,
    pub plaintext_export_path: PathBuf,
}

impl Config {
    pub fn host_builder(host: &str, port: u16) -> uri::Builder {
        let authority = if port != 443 {
            format!("{}:{}", host, port)
        } else {
            host.to_owned()
        };

        let authority = Authority::from_str(&authority).unwrap();
        Uri::builder().scheme("https").authority(authority)
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            database_path: PathBuf::from("isixhosa_click.db"),
            tantivy_path: PathBuf::from("tantivy_data/"),
            cert_path: Some(PathBuf::from("tls/cert.pem")),
            key_path: Some(PathBuf::from("tls/key.rsa")),
            static_site_files: PathBuf::from("static/"),
            other_static_files: PathBuf::from("dummy_www/"),
            log_path: PathBuf::from("log/"),
            http_port: 8080,
            https_port: 8443,
            host: "127.0.0.1".to_string(),
            oidc_client: "DUMMY_CLIENT".to_string(),
            oidc_secret: "DUMMY_SECRET".to_string(),
            plaintext_export_path: PathBuf::from("isixhosa_click_export/"),
        }
    }
}
