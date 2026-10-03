//! Shared HTTP client builder used by every tool that hits the network.

use reqwest::blocking::Client;
use std::time::Duration;

pub fn http_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(25))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .expect("http client")
}
