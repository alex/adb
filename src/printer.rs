use epson::AsyncWriterExt;

pub async fn new_epson_writer() -> anyhow::Result<epson::Writer<impl tokio::io::AsyncWrite>> {
    let stream = tokio::net::TcpStream::connect("192.168.7.238:9100").await?;
    let mut w = epson::Writer::open(epson::Model::T30II, stream).await?;
    w.set_unicode().await?;
    w.speed(5).await?;
    Ok(w)
}
