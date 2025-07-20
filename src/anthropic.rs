#[derive(serde::Serialize)]
struct CompletionRequest<'a> {
    model: &'static str,
    messages: Vec<CompletionRequestMessage<'a>>,
    max_tokens: usize,
}

#[derive(serde::Serialize)]
struct CompletionRequestMessage<'a> {
    role: &'static str,
    content: Vec<MessageContent<'a>>,
}

#[derive(serde::Serialize)]
#[serde(tag = "type")]
pub enum MessageContent<'a> {
    #[serde(rename = "text")]
    Text { text: &'a str },
    #[serde(rename = "image")]
    Image { source: ImageSource<'a> },
}

#[derive(serde::Serialize)]
pub struct ImageSource<'a> {
    #[serde(rename = "type")]
    type_: &'static str,
    data: &'a str,
    media_type: &'static str,
}

impl<'a> ImageSource<'a> {
    pub fn new_base64(media_type: &'static str, data: &'a str) -> Self {
        ImageSource {
            type_: "base64",
            data,
            media_type,
        }
    }
}

#[derive(serde::Deserialize, Debug)]
struct CompletionResponse {
    content: Vec<ResponseContent>,
}

#[derive(serde::Deserialize, Debug)]
struct ResponseContent {
    text: String,
}

pub async fn get_completion(
    client: &reqwest::Client,
    api_token: &str,
    message_contents: impl IntoIterator<Item = MessageContent<'_>>,
) -> anyhow::Result<String> {
    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_token)
        .header("anthropic-version", "2023-06-01")
        .json(&CompletionRequest {
            model: "claude-opus-4-20250514",
            messages: vec![CompletionRequestMessage {
                role: "user",
                content: message_contents.into_iter().collect(),
            }],
            max_tokens: 4096,
        })
        .send()
        .await?
        .json::<CompletionResponse>()
        .await?;

    Ok(response.content[0].text.clone())
}
