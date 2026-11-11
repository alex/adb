use anyhow::Context;
use epson::AsyncWriterExt;

const PRINTER_ADDR: &str = "192.168.4.23:9100";

pub async fn new_epson_writer() -> anyhow::Result<epson::Writer<impl tokio::io::AsyncWrite>> {
    let stream = tokio::net::TcpStream::connect(PRINTER_ADDR)
        .await
        .with_context(|| format!("Failed to connect to printer at {PRINTER_ADDR}"))?;
    let mut w = epson::Writer::open(epson::Model::T30II, stream)
        .await
        .context("Failed to initialize printer")?;
    w.set_unicode().await?;
    w.speed(5).await?;
    Ok(w)
}
