use anyhow::Context;
use base64::Engine;
use clap::Parser;
use epson::AsyncWriterExt;
use image::buffer::ConvertBuffer;

static TODOIST_API_TOKEN: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| std::env::var("TODOIST_API_TOKEN").expect("Missing env var"));
static ANTHROPIC_API_TOKEN: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| std::env::var("ANTHROPIC_API_TOKEN").expect("Missing env var"));

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

async fn new_epson_writer() -> anyhow::Result<epson::Writer<impl tokio::io::AsyncWrite>> {
    let stream = tokio::net::TcpStream::connect("192.168.7.238:9100").await?;
    let mut w = epson::Writer::open(epson::Model::T30II, stream).await?;
    w.set_unicode().await?;

    w.speed(5).await?;

    Ok(w)
}

async fn print_gram_startup_message() -> anyhow::Result<()> {
    let mut w = new_epson_writer().await?;
    let now = chrono::offset::Local::now();

    w.justify(epson::Alignment::Center).await?;
    w.underline(true).await?;
    w.write_all(b"Gram Server Started\n").await?;
    w.underline(false).await?;
    w.write_all(format!("{}\n", now.format("%A %B %d, %Y at %I:%M:%S %p")).as_bytes())
        .await?;
    w.feed(3).await?;
    w.cut().await?;

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

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
        adb::anthropic::get_completion(
            &client,
            &ANTHROPIC_API_TOKEN,
            [adb::anthropic::MessageContent::Text {
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
const PHOTO_HTML: &str = include_str!("photo.html");

struct AppError(anyhow::Error);

impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        tracing::error!("Request failed: {:#}", self.0);
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

#[derive(serde::Deserialize)]
struct PostGramOptions {
    description: Option<bool>,
    rotate_if_landscape: Option<bool>,
}

// CourtListener webhook structures
#[derive(serde::Deserialize, Debug)]
struct CourtListenerWebhook {
    webhook: serde_json::Value,
    payload: WebhookPayload,
}

#[derive(serde::Deserialize, Debug)]
struct WebhookPayload {
    results: Vec<DocketEntry>,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct DocketEntry {
    description: Option<String>,
    entry_number: Option<i32>,
    date_filed: Option<String>,
    docket: Option<serde_json::Value>,
    recap_documents: Option<Vec<RecapDocument>>,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct RecapDocument {
    description: Option<String>,
    document_number: Option<String>,
    attachment_number: Option<i32>,
}

async fn post_gram(
    headers: axum::http::header::HeaderMap,
    axum::extract::Query(opts): axum::extract::Query<PostGramOptions>,
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

    let img = if img.width() > img.height() && opts.rotate_if_landscape.unwrap_or(false) {
        img.rotate90()
    } else {
        img
    };

    let img = img.resize(
        WIDTH.into(),
        4096 * 512,
        image::imageops::FilterType::Lanczos3,
    );
    let img = image::imageops::colorops::brighten(&img, 64);
    let mut img: image::GrayImage = img.convert();
    image::imageops::colorops::dither(&mut img, &image::imageops::colorops::BiLevel);

    let description = if matches!(opts.description, None | Some(true)) {
        let client = reqwest::Client::new();
        Some(adb::anthropic::get_completion(
            &client,
            &ANTHROPIC_API_TOKEN,
            [
                adb::anthropic::MessageContent::Text {
                    text: "Write a short description of what's depicted in the drawing. It should be at most a sentence",
                },
                adb::anthropic::MessageContent::Image {
                    source: adb::anthropic::ImageSource::new_base64("image/png", &base64::prelude::BASE64_STANDARD.encode(&image_post_data))
                },
            ],
        )
        .await?)
    } else {
        None
    };

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
    w.print_image(img).await?;

    if let Some(description) = description {
        w.feed(2).await?;
        w.underline(true).await?;
        w.write_all(b"Description:").await?;
        w.underline(false).await?;
        w.write_all(format!(" {description}\n").as_bytes()).await?;
    }

    w.feed(5).await?;
    w.cut().await?;

    Ok(axum::http::StatusCode::CREATED)
}

async fn check_if_substantive(entry: &DocketEntry) -> anyhow::Result<bool> {
    let client = reqwest::Client::new();

    // Build a description of the docket entry
    let entry_description = format!(
        "Docket Entry:\n\
         Entry Number: {}\n\
         Date Filed: {}\n\
         Description: {}\n\
         Documents: {}",
        entry.entry_number.map(|n| n.to_string()).unwrap_or_else(|| "N/A".to_string()),
        entry.date_filed.as_deref().unwrap_or("N/A"),
        entry.description.as_deref().unwrap_or("N/A"),
        entry.recap_documents.as_ref()
            .map(|docs| docs.iter()
                .filter_map(|d| d.description.as_deref())
                .collect::<Vec<_>>()
                .join("; "))
            .unwrap_or_else(|| "N/A".to_string())
    );

    let prompt = format!(
        "You are analyzing a court docket filing to determine if it is substantive. \
        A substantive filing is one that materially affects the case, such as: motions, \
        orders, opinions, judgments, briefs, complaints, answers, or other significant \
        legal documents. Non-substantive filings include: notices of appearance, \
        administrative orders, scheduling updates, certificate of service, etc.\n\n\
        Analyze this filing and respond with ONLY 'YES' if it is substantive or 'NO' if it is not.\n\n\
        {}",
        entry_description
    );

    let response = adb::anthropic::get_completion(
        &client,
        &ANTHROPIC_API_TOKEN,
        [adb::anthropic::MessageContent::Text { text: &prompt }],
    ).await?;

    // Check if response contains "YES" (case insensitive)
    Ok(response.trim().to_uppercase().contains("YES"))
}

async fn print_docket_alert(entry: &DocketEntry) -> anyhow::Result<()> {
    let mut w = new_epson_writer().await?;
    let now = chrono::offset::Local::now();

    w.justify(epson::Alignment::Center).await?;
    w.underline(true).await?;
    w.write_all(b"COURT ALERT\n").await?;
    w.underline(false).await?;
    w.write_all(b"Substantive Filing Detected\n").await?;
    w.feed(1).await?;
    w.justify(epson::Alignment::Left).await?;

    w.write_all(format!("Alert Time: {}\n", now.format("%B %d, %Y at %I:%M:%S %p")).as_bytes()).await?;
    w.feed(1).await?;

    if let Some(entry_num) = entry.entry_number {
        w.underline(true).await?;
        w.write_all(b"Entry Number:").await?;
        w.underline(false).await?;
        w.write_all(format!(" {}\n", entry_num).as_bytes()).await?;
    }

    if let Some(date_filed) = &entry.date_filed {
        w.underline(true).await?;
        w.write_all(b"Date Filed:").await?;
        w.underline(false).await?;
        w.write_all(format!(" {}\n", date_filed).as_bytes()).await?;
    }

    if let Some(description) = &entry.description {
        w.feed(1).await?;
        w.underline(true).await?;
        w.write_all(b"Description:\n").await?;
        w.underline(false).await?;
        w.write_all(format!("{}\n", description).as_bytes()).await?;
    }

    // Print document descriptions if available
    if let Some(docs) = &entry.recap_documents {
        if !docs.is_empty() {
            w.feed(1).await?;
            w.underline(true).await?;
            w.write_all(b"Documents:\n").await?;
            w.underline(false).await?;

            for doc in docs {
                if let Some(doc_desc) = &doc.description {
                    let doc_num = doc.document_number.as_deref().unwrap_or("?");
                    w.write_all(format!("- Doc {}: {}\n", doc_num, doc_desc).as_bytes()).await?;
                }
            }
        }
    }

    w.feed(3).await?;
    w.cut().await?;

    Ok(())
}

async fn post_courtlistener_webhook(
    axum::Json(webhook): axum::Json<CourtListenerWebhook>,
) -> Result<axum::http::StatusCode, AppError> {
    tracing::info!("Received CourtListener webhook with {} docket entries",
                   webhook.payload.results.len());

    for entry in &webhook.payload.results {
        tracing::info!("Processing docket entry: {:?}", entry.entry_number);

        match check_if_substantive(entry).await {
            Ok(true) => {
                tracing::info!("Entry is substantive, printing alert");
                if let Err(e) = print_docket_alert(entry).await {
                    tracing::error!("Failed to print alert: {:#}", e);
                }
            }
            Ok(false) => {
                tracing::info!("Entry is not substantive, skipping");
            }
            Err(e) => {
                tracing::error!("Failed to check if entry is substantive: {:#}", e);
            }
        }
    }

    Ok(axum::http::StatusCode::OK)
}

async fn gram() -> anyhow::Result<()> {
    print_gram_startup_message().await?;

    let app = axum::Router::new()
        .route(
            "/",
            axum::routing::get(|| async { axum::response::Html(DRAWING_HTML) }),
        )
        .route(
            "/photo/",
            axum::routing::get(|| async { axum::response::Html(PHOTO_HTML) }),
        )
        .route("/gram/", axum::routing::post(post_gram))
        .route("/courtlistener/webhook/", axum::routing::post(post_courtlistener_webhook))
        .layer(axum::extract::DefaultBodyLimit::max(50 * 1024 * 1024));

    let addr = "0.0.0.0:3000";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("Listening on {addr}");
    axum::serve(listener, app).await?;

    Ok(())
}
