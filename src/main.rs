use anyhow::Context;
use base64::Engine;
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

async fn new_epson_writer() -> anyhow::Result<epson::AsyncWriter> {
    let stream = tokio::net::TcpStream::connect("192.168.7.238:9100").await?;
    let mut w = epson::AsyncWriter::open(epson::Model::T30II, Box::new(stream)).await?;
    w.set_unicode().await?;

    w.speed(5).await?;

    Ok(w)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Adb => adb().await,
        Commands::Gram => gram().await,
    }
}

async fn adb() -> anyhow::Result<()> {
    let mut w = new_epson_writer().await?;
    let today = chrono::offset::Local::now();

    let client = reqwest::Client::new();

    let us_history_prompt = format!(
        "Select a major US history fact that happened on today's date ({}) and write a one paragraph summary of it. Favor facts which are related to either democracy, law, science, or technology. Make your write up focus on the facts of what happened and minimize flowery language. Omit context that a smart, well-educated person, will already know. Do not include any text besides the one paragraph. Ensure it is accurate.",
        today.format("%B %d")
    );
    let weather_fut = async {
        adb::weather::get_weather(&client, 38.9067, -77.0279)
            .await
            .context("Error encountered getting weather")
    };
    let todo_fut = async {
        adb::todoist::get_todo_items(&client, &TODOIST_API_TOKEN)
            .await
            .context("Error encountered getting TODO items")
    };
    let us_history_fact_fut = async {
        adb::openai::get_completion(
            &client,
            &OPENAI_API_TOKEN,
            [adb::openai::MessageContent::Text {
                text: &us_history_prompt,
            }],
        )
        .await
        .context("Error encountered getting US history fact")
    };
    let (weather, todo_items, us_history_fact) =
        tokio::try_join!(weather_fut, todo_fut, us_history_fact_fut,)?;

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

const DRAWING_HTML: &str = include_str!("drawing.html");

struct AppError(anyhow::Error);

impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong! {0}", self.0),
        )
            .into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

const WIDTH: u16 = 576;

async fn post_gram(
    headers: axum::http::header::HeaderMap,
    mut form: axum::extract::Multipart,
) -> Result<axum::http::StatusCode, AppError> {
    let now = chrono::offset::Local::now();

    let mut image_post_data = None;
    while let Some(field) = form.next_field().await? {
        if field.name() != Some("image") {
            continue;
        }
        image_post_data = Some(field.bytes().await?);
    }
    let Some(image_post_data) = image_post_data else {
        return Ok(axum::http::StatusCode::BAD_REQUEST);
    };
    let img = image::load_from_memory_with_format(&image_post_data, image::ImageFormat::Png)?;
    let img = img.resize(
        WIDTH.into(),
        4096 * 512,
        image::imageops::FilterType::Lanczos3,
    );

    let client = reqwest::Client::new();
    let png_data_url = "data:image/png;base64,".to_string()
        + &base64::prelude::BASE64_STANDARD.encode(&image_post_data);
    let description = adb::openai::get_completion(
        &client,
        &OPENAI_API_TOKEN,
        [
            adb::openai::MessageContent::Text {
                text: "Write a short description of what's depicted in the drawing. It should be at most a sentence",
            },
            adb::openai::MessageContent::ImageUrl {
                image_url: adb::openai::ImageUrl { url: &png_data_url },
            },
        ],
    )
    .await?;

    let mut w = new_epson_writer().await?;
    w.justify(epson::Alignment::Center).await?;
    w.underline(true).await?;
    w.write_all(b"Gram\n").await?;
    w.underline(false).await?;
    w.justify(epson::Alignment::Left).await?;

    w.justify(epson::Alignment::Center).await?;
    w.write_all(format!("{}\n", now.format("%A %B %d, %Y")).as_bytes())
        .await?;
    w.justify(epson::Alignment::Left).await?;

    w.feed(2).await?;
    w.underline(true).await?;
    w.write_all(b"Received At:").await?;
    w.underline(false).await?;
    w.write_all(format!(" {}\n", now.format("%I:%M:%S %p")).as_bytes())
        .await?;

    if let Some(peer_ip) = headers.get("X-Forwarded-For") {
        let peer_ip = peer_ip.to_str()?;
        w.underline(true).await?;
        w.write_all(b"Peer IP:").await?;
        w.underline(false).await?;
        w.write_all(format!(" {peer_ip}\n").as_bytes()).await?;
    }
    if let Some(user_name) = headers.get("X-Gram-User") {
        let user_name = user_name.to_str()?;
        w.underline(true).await?;
        w.write_all(b"User:").await?;
        w.underline(false).await?;
        w.write_all(format!(" {user_name}\n").as_bytes()).await?;
    }

    w.feed(2).await?;
    w.print_image(img.into()).await?;

    w.feed(2).await?;
    w.underline(true).await?;
    w.write_all(b"Description:").await?;
    w.underline(false).await?;
    w.write_all(format!(" {description}\n").as_bytes()).await?;

    w.feed(5).await?;
    w.cut().await?;

    Ok(axum::http::StatusCode::CREATED)
}

async fn gram() -> anyhow::Result<()> {
    let app = axum::Router::new()
        .route(
            "/",
            axum::routing::get(|| async { axum::response::Html(DRAWING_HTML) }),
        )
        .route("/gram/", axum::routing::post(post_gram));

    let addr = "0.0.0.0:3000";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("Listening on {addr}");
    axum::serve(listener, app).await?;

    Ok(())
}
