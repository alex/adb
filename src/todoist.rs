#[derive(serde::Deserialize)]
struct TodoistTask {
    content: String,
}

pub fn get_todo_items(
    client: &reqwest::blocking::Client,
    api_token: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let response = client
        .get("https://api.todoist.com/rest/v2/tasks?filter=od%20%7C%20due%3Atoday")
        .bearer_auth(api_token)
        .send()?;
    Ok(response
        .json::<Vec<TodoistTask>>()?
        .into_iter()
        .map(|t| t.content)
        .collect())
}
