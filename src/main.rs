use clap::Parser;

static TODOIST_API_TOKEN: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| std::env::var("TODOIST_API_TOKEN").expect("Missing env var"));
static OPENAI_API_TOKEN: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| std::env::var("OPENAI_API_TOKEN").expect("Missing env var"));

#[derive(clap::Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    Adb,
    Gram,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let stream = tokio::net::TcpStream::connect("192.168.7.238:9100").await?;
    let mut w = epson::AsyncWriter::open(epson::Model::T30II, Box::new(stream)).await?;
    w.set_unicode().await?;

    w.speed(5).await?;

    match cli.command {
        Commands::Adb => adb(w).await,
        Commands::Gram => gram(w).await,
    }
}

async fn adb(mut w: epson::AsyncWriter) -> anyhow::Result<()> {
    let today = chrono::offset::Local::now();

    let client = reqwest::blocking::Client::new();
    println!("Fetching weather...");
    let weather = adb::weather::get_weather(&client, 38.9067, -77.0279)?;
    println!("Fetching todo...");
    let todo_items = adb::todoist::get_todo_items(&client, &TODOIST_API_TOKEN)?;
    println!("Fetching us history fact...");
    let us_history_fact = adb::openai::get_completion(&client, &OPENAI_API_TOKEN, &format!("Select a major US history fact that happened on today's date ({}) and write a one paragraph summary of it. Favor facts which are related to either democracy, law, science, or technology. Make your write up focus on the facts of what happened and minimize flowery language. Omit context that a smart, well-educated person, will already know. Do not include any text besides the one paragraph. Ensure it is accurate.", today.format("%B %d")))?;

    w.justify(epson::Alignment::Center).await?;
    w.underline(true).await?;
    w.write_all(b"Alex's Daily Brief\n").await?;
    w.underline(false).await?;
    w.justify(epson::Alignment::Left).await?;

    w.justify(epson::Alignment::Center).await?;
    w.write_all(format!("{}\n", today.format("%A %B %d, %Y")).as_bytes())
        .await?;
    w.justify(epson::Alignment::Left).await?;

    w.feed(2).await?;
    w.underline(true).await?;
    w.write_all(b"Weather Forecast:\n").await?;
    w.underline(false).await?;
    for (day, forecast) in weather {
        w.write_all(format!("{}: {}\n", day, forecast).as_bytes())
            .await?;
    }

    w.feed(2).await?;
    w.underline(true).await?;
    w.write_all(b"TODO:\n").await?;
    w.underline(false).await?;

    for todo in todo_items {
        w.write_all(format!("[ ] {}\n", todo).as_bytes()).await?;
    }

    w.feed(2).await?;
    w.underline(true).await?;
    w.write_all(b"US History Fact:\n").await?;
    w.underline(false).await?;
    w.write_all(us_history_fact.as_bytes()).await?;

    w.feed(5).await?;
    w.cut().await?;

    Ok(())
}

async fn gram(_w: epson::AsyncWriter) -> anyhow::Result<()> {
    todo!()
}
