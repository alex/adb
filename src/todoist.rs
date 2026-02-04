#[derive(serde::Deserialize)]
struct TodoistResponse {
    results: Vec<TodoistTask>,
}

#[derive(serde::Deserialize)]
struct TodoistTask {
    content: String,
}

pub async fn get_todo_items(
    client: &reqwest::Client,
    api_token: &str,
) -> anyhow::Result<Vec<String>> {
    let response = client
        .get("https://api.todoist.com/api/v1/tasks/filter?query=od%20%7C%20due%3Atoday")
        .bearer_auth(api_token)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "todoist API returned HTTP error. Status code={}",
            response.status()
        ));
    }

    Ok(response
        .json::<TodoistResponse>()
        .await?
        .results
        .into_iter()
        .map(|t| t.content)
        .collect())
}
