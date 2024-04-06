use std::io::Write;
use std::net::TcpStream;

// TODO!
const TODOIST_API_TOKEN: &str = env!("TODOIST_API_TOKEN");

#[derive(serde::Deserialize)]
struct TodoistTask {
    content: String,
}

fn get_todo_items(
    client: &reqwest::blocking::Client,
    api_token: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let response = client
        .get("https://api.todoist.com/rest/v2/tasks?filter=od%20%7C%20due%3Atoday")
        .bearer_auth(api_token)
        .send()?;
    Ok(response
        .json::<Vec<TodoistTask>>()?
        .into_iter()
        .map(|t| t.content)
        .collect())
}

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

fn get_weather(
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::new();
    let weather = get_weather(&client, 38.9067, -77.0279)?;
    let todo_items = get_todo_items(&client, TODOIST_API_TOKEN)?;

    let stream = TcpStream::connect("192.168.7.238:9100")?;
    let mut w = epson::Writer::open(epson::Model::T30II, Box::new(stream))?;
    w.set_unicode()?;

    w.speed(5)?;

    w.justify(epson::Alignment::Center)?;
    w.underline(true)?;
    w.write_all(b"Alex's Daily Brief\n")?;
    w.underline(false)?;
    w.justify(epson::Alignment::Left)?;

    w.justify(epson::Alignment::Center)?;
    let t = chrono::offset::Local::now();
    w.write_all(format!("{}\n", t.format("%A %B %d, %Y")).as_bytes())?;
    w.justify(epson::Alignment::Left)?;

    w.feed(2)?;
    w.underline(true)?;
    w.write_all(b"Weather Forecast:\n")?;
    w.underline(false)?;
    for (day, forecast) in weather {
        w.write_all(format!("{}: {}\n", day, forecast).as_bytes())?;
    }

    w.feed(2)?;
    w.underline(true)?;
    w.write_all(b"TODO:\n")?;
    w.underline(false)?;

    for todo in todo_items {
        w.write_all(format!("☐ {}\n", todo).as_bytes())?;
    }

    w.feed(5)?;
    w.cut()?;

    Ok(())
}
