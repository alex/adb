use epson::AsyncWriterExt;

// CourtListener webhook structures
#[derive(serde::Deserialize, Debug)]
pub struct CourtListenerWebhook {
    payload: WebhookPayload,
}

#[derive(serde::Deserialize, Debug)]
pub struct WebhookPayload {
    results: Vec<DocketEntry>,
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct DocketEntry {
    description: Option<String>,
    entry_number: Option<i32>,
    date_filed: Option<String>,
    recap_documents: Option<Vec<RecapDocument>>,
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct RecapDocument {
    description: Option<String>,
    document_number: Option<String>,
}

async fn check_if_substantive(
    client: &reqwest::Client,
    api_token: &str,
    entry: &DocketEntry,
) -> anyhow::Result<bool> {
    // Build a description of the docket entry
    let entry_description = format!(
        "Docket Entry:\n\
         Entry Number: {}\n\
         Date Filed: {}\n\
         Description: {}\n\
         Documents: {}",
        entry
            .entry_number
            .map(|n| n.to_string())
            .unwrap_or_else(|| "N/A".to_string()),
        entry.date_filed.as_deref().unwrap_or("N/A"),
        entry.description.as_deref().unwrap_or("N/A"),
        entry
            .recap_documents
            .as_ref()
            .map(|docs| docs
                .iter()
                .filter_map(|d| d.description.as_deref())
                .collect::<Vec<_>>()
                .join("; "))
            .unwrap_or_else(|| "N/A".to_string())
    );

    let prompt = format!(
        "You are analyzing a court docket filing to determine if it is substantive. \
        A substantive filing is one that materially affects the case, such as: motions, \
        orders, opinions, judgments, briefs, complaints, answers, or other significant \
        legal documents. Non-substantive filings include: notices of appearance, \
        administrative orders, scheduling updates, certificate of service, \
        motions and orders regarding pro hac vice, and motions and orders \
        regarding leave to file amicus curiae brief, etc.\n\n\
        Analyze this filing and respond with ONLY 'YES' if it is substantive or 'NO' if it is not.\n\n\
        {}",
        entry_description
    );

    let response = crate::anthropic::get_completion(
        client,
        api_token,
        [crate::anthropic::MessageContent::Text { text: &prompt }],
    )
    .await?;

    // Check if response contains "YES" (case insensitive)
    Ok(response.trim().to_uppercase().contains("YES"))
}

async fn print_docket_alerts(entries: &[DocketEntry]) -> anyhow::Result<()> {
    let mut w = crate::printer::new_epson_writer().await?;
    let now = chrono::offset::Local::now();

    w.justify(epson::Alignment::Center).await?;
    w.underline(true).await?;
    w.write_all(b"COURT ALERT\n").await?;
    w.underline(false).await?;

    let filing_word = if entries.len() == 1 {
        "Filing"
    } else {
        "Filings"
    };
    w.write_all(format!("{} Substantive {} Detected\n", entries.len(), filing_word).as_bytes())
        .await?;
    w.feed(1).await?;
    w.justify(epson::Alignment::Left).await?;

    w.write_all(format!("Alert Time: {}\n", now.format("%B %d, %Y at %I:%M:%S %p")).as_bytes())
        .await?;

    // Print each entry
    for (i, entry) in entries.iter().enumerate() {
        if i > 0 {
            w.feed(2).await?;
            w.write_all(b"---\n").await?;
            w.feed(1).await?;
        } else {
            w.feed(1).await?;
        }

        if let Some(entry_num) = entry.entry_number {
            w.underline(true).await?;
            w.write_all(b"Entry Number:").await?;
            w.underline(false).await?;
            w.write_all(format!(" {}\n", entry_num).as_bytes()).await?;
        }

        if let Some(date_filed) = &entry.date_filed {
            w.underline(true).await?;
            w.write_all(b"Date Filed:").await?;
            w.underline(false).await?;
            w.write_all(format!(" {}\n", date_filed).as_bytes()).await?;
        }

        if let Some(description) = &entry.description {
            w.feed(1).await?;
            w.underline(true).await?;
            w.write_all(b"Description:\n").await?;
            w.underline(false).await?;
            w.write_all(format!("{}\n", description).as_bytes()).await?;
        }

        // Print document descriptions if available
        if let Some(docs) = &entry.recap_documents {
            if !docs.is_empty()
                && docs
                    .iter()
                    .filter_map(|doc| doc.description.as_ref())
                    .filter(|desc| !desc.trim().is_empty())
                    .count()
                    > 0
            {
                w.feed(1).await?;
                w.underline(true).await?;
                w.write_all(b"Documents:\n").await?;
                w.underline(false).await?;

                for doc in docs {
                    if let Some(doc_desc) = &doc.description {
                        if doc_desc.trim().is_empty() {
                            continue;
                        }
                        let doc_num = doc.document_number.as_deref().unwrap_or("?");
                        w.write_all(format!("- Doc {}: {}\n", doc_num, doc_desc).as_bytes())
                            .await?;
                    }
                }
            }
        }
    }

    w.feed(3).await?;
    w.cut().await?;

    Ok(())
}

pub async fn handle_webhook(
    client: &reqwest::Client,
    api_token: &'static str,
    webhook: CourtListenerWebhook,
) -> anyhow::Result<()> {
    let client = client.clone();
    // courtlistener has a 2 second timeout and talking to an LLM + printing on
    // a printer can take longer than that, so we spawn a background task.
    tokio::spawn(async move {
        // Check all entries and collect substantive ones
        let mut substantive_entries = Vec::new();

        for entry in webhook.payload.results {
            if check_if_substantive(&client, api_token, &entry)
                .await
                .unwrap()
            {
                substantive_entries.push(entry);
            }
        }

        // Only print if there are substantive entries
        if !substantive_entries.is_empty() {
            print_docket_alerts(&substantive_entries).await.unwrap();
        }
    });

    Ok(())
}
