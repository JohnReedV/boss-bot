use crate::resources::*;
use crate::utils::*;
use crate::Handler;
use ollama_rs::generation::completion::request::GenerationRequest;
use ollama_rs::Ollama;
use rand::random;
use reqwest::Client;
use serde_json::json;
use serde_json::Value;
use serenity::{
    model::{
        channel::Message,
        prelude::{GuildId, ReactionType},
    },
    prelude::Context,
};
use songbird::Songbird;
use std::io::Error;
use std::{
    collections::VecDeque,
    sync::{atomic::Ordering, Arc},
};
use tokio::{
    fs,
    time::{sleep, Duration},
};

pub async fn skip_all_enabled(app: &Handler, guild_id: GuildId, manager: Arc<Songbird>) {
    if let Some(handler_lock) = manager.get(guild_id) {
        let mut handler = handler_lock.lock().await;
        handler.stop();
    }
    {
        let playing_lock = app.playing.lock().await;
        if *playing_lock {
            app.skip_player.store(true, Ordering::SeqCst);
        }
    }
    {
        let tracking_lock = app.tracking.lock().await;
        if *tracking_lock {
            app.skip_tracker.store(true, Ordering::SeqCst);
        }
    }
    {
        let looping_lock = app.looping.lock().await;
        if *looping_lock {
            app.skip_loop.store(true, Ordering::SeqCst);
        }
    }
}

pub async fn ollama(prompt: String) -> String {
    let ollama = Ollama::default();
    let model = MODEL.to_string();

    let gen_req = GenerationRequest::new(model, prompt).system(DEFAULT_SYSTEM_MOCK.to_string());

    let res = ollama.generate(gen_req).await.unwrap();
    println!("->> res {}", res.response);

    return res.response;
}

pub async fn create_image(prompt: String) -> Result<u64, reqwest::Error> {
    let client = Client::new();
    let url = format!("http://127.0.0.1:8188/prompt");

    let workflow_json_str = fs::read_to_string("./workflow_api.json").await.unwrap();
    let mut workflow_json: Value = serde_json::from_str(&workflow_json_str).unwrap();

    let seed: u64 = random();
    if let Some(node) = workflow_json.get_mut("3") {
        if let Some(inputs) = node.get_mut("inputs") {
            inputs["seed"] = json!(seed);
        }
    }

    if let Some(node) = workflow_json.get_mut("6") {
        if let Some(inputs) = node.get_mut("inputs") {
            inputs["text"] = json!(prompt);
        }
    }

    if let Some(node) = workflow_json.get_mut("9") {
        if let Some(inputs) = node.get_mut("inputs") {
            inputs["filename_prefix"] = json!(seed);
        }
    }

    match client
        .post(&url)
        .json(&json!({
            "prompt": workflow_json,
            "client_id": "4",
        }))
        .send()
        .await
    {
        Ok(_) => Ok(seed),
        Err(err) => Err(err),
    }
}

pub async fn get_image_path(name: String) -> Result<String, Error> {
    let path: &str = "../ComfyUI/output";
    let mut entries = fs::read_dir(path).await?;

    while let Some(entry) = entries.next_entry().await? {
        let file_name = entry.file_name().into_string().unwrap_or_default();
        let path = entry.path().display().to_string();

        if file_name.starts_with(name.as_str()) {
            return Ok(path);
        }
    }

    Err(Error::new(
        std::io::ErrorKind::NotFound,
        format!("Error with prefix {} not found", name),
    ))
}

pub async fn dalle_image(ctx: Context, msg: Message, api_key: &str, prompt: &str) -> String {
    let client = Client::new();
    let url = "https://api.openai.com/v1/images/generations";

    let bot_msg = msg
        .reply(&ctx, "Select a size")
        .await
        .expect("Expected prompt message");

    let sizes = vec!["🟦", "↔", "↕️"];
    for emoji in &sizes {
        bot_msg
            .react(&ctx.http, ReactionType::Unicode(emoji.to_string()))
            .await
            .expect("Expected to react");
    }

    let mut selected_size = None;
    while selected_size.is_none() {
        for emoji in &sizes {
            let reaction_type = ReactionType::Unicode(emoji.to_string());
            if let Ok(users) = bot_msg
                .reaction_users(&ctx.http, reaction_type, None, None)
                .await
            {
                if users.iter().any(|user| user.id == msg.author.id) {
                    selected_size = match *emoji {
                        "🟦" => Some("1024x1024"),
                        "↔" => Some("1792x1024"),
                        "↕️" => Some("1024x1792"),
                        _ => None,
                    };
                    break;
                }
            }
        }
        if selected_size.is_none() {
            sleep(Duration::from_secs(1)).await;
        }
    }

    bot_msg.delete(&ctx).await.expect("expected to delete");
    let size = selected_size.unwrap().to_string();

    let bot_msg_2 = msg.reply(&ctx, "Generating...").await.unwrap();

    match client
        .post(url)
        .bearer_auth(api_key)
        .json(&serde_json::json!({
            "prompt": prompt,
            "model": "dall-e-3",
            "n": 1,
            "quality": "hd",
            "size": size

        }))
        .send()
        .await
    {
        Ok(response) => match response.text().await {
            Ok(response_body) => {
                bot_msg_2.delete(&ctx).await.unwrap();
                let json: Value = match serde_json::from_str(&response_body) {
                    Ok(json) => json,
                    Err(_) => return "Failed to parse JSON".to_string(),
                };

                return json["data"][0]["url"]
                    .as_str()
                    .unwrap_or(json["error"]["message"].to_string().as_str())
                    .to_string();
            }
            Err(_) => {
                bot_msg_2.delete(&ctx).await.unwrap();
                return "Failed to get response text from JSON".to_string();
            }
        },
        Err(_) => {
            bot_msg_2.delete(&ctx).await.unwrap();
            return "Failed to send request".to_string();
        }
    }
}

pub async fn say_queue(msg: Message, ctx: &Context, queue: VecDeque<Node>) {
    let mut name_str = String::from("🎵 **Queue** 🎵\n```markdown\n");
    let mut tracker = false;

    for (index, item) in queue.iter().enumerate() {
        tracker = true;
        let title = get_video_title(&item.url).await.unwrap();
        let final_title = title.trim();
        name_str.push_str(&format!("{}: {}\n", index + 1, final_title));
    }
    name_str.push_str("```");

    if tracker {
        msg.channel_id.say(&ctx.http, name_str).await.unwrap();
    } else {
        msg.channel_id
            .say(&ctx.http, "🪹 **Queue Empty** 🪹")
            .await
            .unwrap();
    }
}
