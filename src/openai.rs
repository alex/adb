#[derive(serde::Serialize)]
struct CompletionRequest<'a> {
    model: &'static str,
    messages: Vec<CompletionRequestMessage<'a>>,
}

#[derive(serde::Serialize)]
struct CompletionRequestMessage<'a> {
    role: &'static str,
    content: MessageContent<'a>,
}

#[derive(serde::Serialize)]
#[serde(tag = "type")]
pub enum MessageContent<'a> {
    #[serde(rename = "text")]
    Text { text: &'a str },

    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrl<'a> },
}

#[derive(serde::Serialize)]
pub struct ImageUrl<'a> {
    url: &'a str,
}

#[derive(serde::Deserialize, Debug)]
struct CompletionResponse {
    choices: Vec<CompletionResponseChoice>,
}

#[derive(serde::Deserialize, Debug)]
struct CompletionResponseChoice {
    message: CompletionResponseChoiceMessage,
}

#[derive(serde::Deserialize, Debug)]
struct CompletionResponseChoiceMessage {
    content: String,
}

pub async fn get_completion(
    client: &reqwest::Client,
    api_token: &str,
    message_contents: impl IntoIterator<Item = MessageContent<'_>>,
) -> anyhow::Result<String> {
    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_token)
        .json(&CompletionRequest {
            model: "gpt-4o",
            messages: message_contents
                .into_iter()
                .map(|content| CompletionRequestMessage {
                    role: "user",
                    content,
                })
                .collect(),
        })
        .send()
        .await?
        .json::<CompletionResponse>()
        .await?;

    Ok(response.choices[0].message.content.clone())
}
