//! `get_weather`: geocode a location via Open-Meteo, then fetch
//! current conditions plus a 3-day forecast.

use serde_json::Value;
use std::time::Duration;

use crate::consts::VERSION;
use crate::http::http_client;

fn weather_code_to_text(code: i64) -> &'static str {
    match code {
        0 => "Clear sky",
        1 => "Mainly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51 | 53 | 55 => "Drizzle",
        56 | 57 => "Freezing drizzle",
        61 | 63 | 65 => "Rain",
        66 | 67 => "Freezing rain",
        71 | 73 | 75 => "Snowfall",
        77 => "Snow grains",
        80 | 81 | 82 => "Rain showers",
        85 | 86 => "Snow showers",
        95 => "Thunderstorm",
        96 | 99 => "Thunderstorm with hail",
        _ => "Unknown conditions",
    }
}

pub fn tool_get_weather(location: &str) -> String {
    if location.is_empty() {
        return String::from("No location supplied.");
    }
    let client = http_client();
    let geo: Value = match client
        .get("https://geocoding-api.open-meteo.com/v1/search")
        .header("User-Agent", format!("Pocai/{}", VERSION))
        .query(&[("name", location), ("count", "1")])
        .timeout(Duration::from_secs(15))
        .send()
    {
        Ok(r) => {
            if !r.status().is_success() {
                return String::from(
                    "Weather lookup failed: could not reach the geocoding service.",
                );
            }
            match r.json() {
                Ok(j) => j,
                Err(_) => {
                    return String::from(
                        "Weather lookup failed: could not reach the geocoding service.",
                    )
                }
            }
        }
        Err(_) => {
            return String::from("Weather lookup failed: could not reach the geocoding service.")
        }
    };

    let lat = geo
        .pointer("/results/0/latitude")
        .and_then(|v| v.as_f64())
        .map(|f| f.to_string())
        .unwrap_or_default();
    let lon = geo
        .pointer("/results/0/longitude")
        .and_then(|v| v.as_f64())
        .map(|f| f.to_string())
        .unwrap_or_default();
    let place = geo
        .pointer("/results/0/name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let country = geo
        .pointer("/results/0/country")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if lat.is_empty() || lon.is_empty() {
        return format!(
            "Weather lookup failed: could not find a location matching \"{}\".",
            location
        );
    }

    let weather: Value = match client
        .get("https://api.open-meteo.com/v1/forecast")
        .header("User-Agent", format!("Pocai/{}", VERSION))
        .query(&[
            ("latitude", lat.as_str()),
            ("longitude", lon.as_str()),
            (
                "current",
                "temperature_2m,relative_humidity_2m,apparent_temperature,precipitation,weather_code,wind_speed_10m",
            ),
            (
                "daily",
                "temperature_2m_max,temperature_2m_min,precipitation_probability_max,weather_code",
            ),
            ("forecast_days", "3"),
            ("timezone", "auto"),
        ])
        .timeout(Duration::from_secs(15))
        .send()
    {
        Ok(r) => {
            if !r.status().is_success() {
                return String::from(
                    "Weather lookup failed: could not reach the forecast service.",
                );
            }
            match r.json() {
                Ok(j) => j,
                Err(_) => {
                    return String::from(
                        "Weather lookup failed: could not reach the forecast service.",
                    )
                }
            }
        }
        Err(_) => {
            return String::from(
                "Weather lookup failed: could not reach the forecast service.",
            )
        }
    };

    let str_at = |ptr: &str| -> String {
        weather
            .pointer(ptr)
            .map(|v| {
                if v.is_string() {
                    v.as_str().unwrap_or("?").to_string()
                } else {
                    v.to_string().trim_matches('"').to_string()
                }
            })
            .unwrap_or_else(|| String::from("?"))
    };
    let temp = str_at("/current/temperature_2m");
    let feels = str_at("/current/apparent_temperature");
    let humidity = str_at("/current/relative_humidity_2m");
    let precip = str_at("/current/precipitation");
    let wind = str_at("/current/wind_speed_10m");
    let code: i64 = weather
        .pointer("/current/weather_code")
        .and_then(|v| v.as_i64())
        .unwrap_or(-1);
    let condition = weather_code_to_text(code);

    let mut out = String::new();
    if country.is_empty() {
        out.push_str(&format!("Location: {}\n", place));
    } else {
        out.push_str(&format!("Location: {}, {}\n", place, country));
    }
    out.push_str(&format!("Current conditions: {}\n", condition));
    out.push_str(&format!(
        "Temperature: {}°C (feels like {}°C)\n",
        temp, feels
    ));
    out.push_str(&format!("Humidity: {}%\n", humidity));
    out.push_str(&format!("Precipitation: {} mm\n", precip));
    out.push_str(&format!("Wind speed: {} km/h\n\n", wind));
    out.push_str("3-day forecast:\n");

    let days = weather
        .pointer("/daily/time")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    for i in 0..days {
        let d = weather
            .pointer(&format!("/daily/time/{}", i))
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let hi = weather
            .pointer(&format!("/daily/temperature_2m_max/{}", i))
            .map(|v| v.to_string())
            .unwrap_or_else(|| String::from("?"));
        let lo = weather
            .pointer(&format!("/daily/temperature_2m_min/{}", i))
            .map(|v| v.to_string())
            .unwrap_or_else(|| String::from("?"));
        let pprob = weather
            .pointer(&format!("/daily/precipitation_probability_max/{}", i))
            .map(|v| v.to_string())
            .unwrap_or_else(|| String::from("?"));
        let pcode: i64 = weather
            .pointer(&format!("/daily/weather_code/{}", i))
            .and_then(|v| v.as_i64())
            .unwrap_or(-1);
        let pcond = weather_code_to_text(pcode);
        out.push_str(&format!(
            "  {}: {}, high {}°C / low {}°C, {}% chance of precipitation\n",
            d, pcond, hi, lo, pprob
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_codes() {
        assert_eq!(weather_code_to_text(0), "Clear sky");
        assert_eq!(weather_code_to_text(95), "Thunderstorm");
        assert_eq!(weather_code_to_text(999), "Unknown conditions");
    }
}
