#[derive(serde::Deserialize)]
struct TodoistTask {
    content: String,
}

pub async fn get_todo_items(
    client: &reqwest::Client,
    api_token: &str,
) -> anyhow::Result<Vec<String>> {
    let response = client
        .get("https://api.todoist.com/rest/v2/tasks?filter=od%20%7C%20due%3Atoday")
        .bearer_auth(api_token)
        .send()
        .await?;
    Ok(response
        .json::<Vec<TodoistTask>>()
        .await?
        .into_iter()
        .map(|t| t.content)
        .collect())
}
