use std::io::Write;
use std::net::TcpStream;

static TODOIST_API_TOKEN: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| std::env::var("TODOIST_API_TOKEN").expect("Missing env var"));
static OPENAI_API_TOKEN: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| std::env::var("OPENAI_API_TOKEN").expect("Missing env var"));

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let today = chrono::offset::Local::now();

    let client = reqwest::blocking::Client::new();
    println!("Fetching weather...");
    let weather = adb::weather::get_weather(&client, 38.9067, -77.0279)?;
    println!("Fetching todo...");
    let todo_items = adb::todoist::get_todo_items(&client, &TODOIST_API_TOKEN)?;
    println!("Fetching us history fact...");
    let us_history_fact = adb::openai::get_completion(&client, &OPENAI_API_TOKEN, &format!("Select a major US history fact that happened on today's date ({}) and write a one paragraph summary of it. Favor facts which are related to either democracy, law, science, or technology. Make your write up focus on the facts of what happened and minimize flowery language. Omit context that a smart, well-educated person, will already know. Do not include any text besides the one paragraph. Ensure it is accurate.", today.format("%B %d")))?;

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
    w.write_all(format!("{}\n", today.format("%A %B %d, %Y")).as_bytes())?;
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
        w.write_all(format!("[ ] {}\n", todo).as_bytes())?;
    }

    w.feed(2)?;
    w.underline(true)?;
    w.write_all(b"US History Fact:\n")?;
    w.underline(false)?;
    w.write_all(us_history_fact.as_bytes())?;

    w.feed(5)?;
    w.cut()?;

    Ok(())
}
