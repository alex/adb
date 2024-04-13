#[derive(serde::Deserialize)]
struct PointResponse {
    properties: PointProperties,
}

#[derive(serde::Deserialize)]
struct PointProperties {
    forecast: String,
}

#[derive(serde::Deserialize, Debug)]
struct ForecastResponse {
    properties: ForecastProperties,
}

#[derive(serde::Deserialize, Debug)]
struct ForecastProperties {
    periods: Vec<ForecastPeriod>,
}

#[derive(serde::Deserialize, Debug)]
struct ForecastPeriod {
    name: String,
    #[serde(rename = "isDaytime")]
    is_daytime: bool,
    temperature: i32,
    #[serde(rename = "shortForecast")]
    short_forecast: String,
}

pub fn get_weather(
    client: &reqwest::blocking::Client,
    lat: f64,
    lon: f64,
) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let point_response = client
        .get(format!("https://api.weather.gov/points/{lat},{lon}"))
        .header(reqwest::header::USER_AGENT, "adb/0.1.0")
        .send()?
        .json::<PointResponse>()?;

    let forecast_response = client
        .get(point_response.properties.forecast)
        .header(reqwest::header::USER_AGENT, "adb/0.1.0")
        .send()?
        .json::<ForecastResponse>()?;

    Ok(forecast_response
        .properties
        .periods
        .into_iter()
        .enumerate()
        .filter_map(|(i, period)| {
            if i > 0 && !period.is_daytime {
                return None;
            }
            Some((
                period.name,
                format!("{}°F, {}", period.temperature, period.short_forecast),
            ))
        })
        .take(3)
        .collect())
}
