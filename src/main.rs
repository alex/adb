use std::sync::Arc;

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

    let client = reqwest::Client::new();

    let us_history_prompt = format!("Select a major US history fact that happened on today's date ({}) and write a one paragraph summary of it. Favor facts which are related to either democracy, law, science, or technology. Make your write up focus on the facts of what happened and minimize flowery language. Omit context that a smart, well-educated person, will already know. Do not include any text besides the one paragraph. Ensure it is accurate.", today.format("%B %d"));
    let (weather, todo_items, us_history_fact) = tokio::try_join!(
        adb::weather::get_weather(&client, 38.9067, -77.0279),
        adb::todoist::get_todo_items(&client, &TODOIST_API_TOKEN),
        adb::openai::get_completion(&client, &OPENAI_API_TOKEN, &us_history_prompt)
    )?;

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

const DRAWING_HTML: &str = include_str!("../drawing.html");

struct AppState {
    w: tokio::sync::Mutex<epson::AsyncWriter>,
}

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
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::ConnectInfo(client_ip): axum::extract::ConnectInfo<std::net::SocketAddr>,
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
        todo!()
    };
    let img = image::load_from_memory_with_format(&image_post_data, image::ImageFormat::Png)?;
    let img = img.resize(
        WIDTH.into(),
        4096 * 512,
        image::imageops::FilterType::Lanczos3,
    );

    let mut w = state.w.lock().await;
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

    w.underline(true).await?;
    w.write_all(b"Peer IP:").await?;
    w.underline(false).await?;
    w.write_all(format!(" {client_ip}\n").as_bytes()).await?;

    w.feed(2).await?;
    w.print_image(img.into()).await?;

    w.feed(5).await?;
    w.cut().await?;

    Ok(axum::http::StatusCode::CREATED)
}

async fn gram(w: epson::AsyncWriter) -> anyhow::Result<()> {
    let app = axum::Router::new()
        .route(
            "/",
            axum::routing::get(|| async { axum::response::Html(DRAWING_HTML) }),
        )
        .route("/gram/", axum::routing::post(post_gram))
        .with_state(Arc::new(AppState {
            w: tokio::sync::Mutex::new(w),
        }));

    let addr = "0.0.0.0:3000";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("Listening on {addr}");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}
