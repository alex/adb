#[derive(serde::Serialize)]
struct CompletionRequest<'a> {
    model: &'static str,
    messages: Vec<CompletionRequestMessage<'a>>,
}

#[derive(serde::Serialize)]
struct CompletionRequestMessage<'a> {
    role: &'static str,
    content: &'a str,
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

pub fn get_completion(
    client: &reqwest::blocking::Client,
    api_token: &str,
    prompt: &str,
) -> anyhow::Result<String> {
    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_token)
        .json(&CompletionRequest {
            model: "gpt-4o",
            messages: vec![CompletionRequestMessage {
                role: "user",
                content: prompt,
            }],
        })
        .send()?
        .json::<CompletionResponse>()?;

    Ok(response.choices[0].message.content.clone())
}
