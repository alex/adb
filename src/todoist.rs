#[derive(serde::Deserialize)]
struct TodoistResponse {
    results: Vec<TodoistTask>,
}

#[derive(serde::Deserialize)]
struct TodoistTask {
    content: String,
    due: Option<TodoistDue>,
}

#[derive(serde::Deserialize)]
struct TodoistDue {
    date: String,
}

pub struct TodoItem {
    pub content: String,
    pub time: Option<chrono::NaiveTime>,
}

fn parse_due_time(s: &str) -> Option<chrono::NaiveTime> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&chrono::Local).time());
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return Some(dt.time());
    }
    None
}

pub async fn get_todo_items(
    client: &reqwest::Client,
    api_token: &str,
) -> anyhow::Result<Vec<TodoItem>> {
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
        .map(|t| TodoItem {
            content: t.content,
            time: t.due.as_ref().and_then(|d| parse_due_time(&d.date)),
        })
        .collect())
}
