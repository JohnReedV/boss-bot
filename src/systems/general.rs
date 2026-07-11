use crate::resources::*;
use crate::utils::*;
use crate::Handler;
use base64::{engine::general_purpose, Engine as _};
use openai_rust::{chat::ChatArguments, chat::Message as OpenAiMessage, Client as OpenAiClient};
use reqwest::Client;
use serde_json::Value;
use serenity::{
    builder::{CreateAttachment, CreateMessage},
    model::{
        channel::Message,
        prelude::{GuildId, ReactionType},
    },
    prelude::Context,
};
use songbird::Songbird;
use std::{
    collections::VecDeque,
    env,
    sync::{atomic::Ordering, Arc},
};
use tokio::time::{sleep, Duration, Instant};

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

pub async fn chat_gpt(api_key: &str, prompt: &str) -> String {
    let client: OpenAiClient = OpenAiClient::new(api_key);
    let model = env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-5.6-sol".to_owned());
    let args: ChatArguments = ChatArguments::new(
        &model,
        vec![OpenAiMessage {
            role: "user".to_owned(),
            content: prompt.to_owned(),
        }],
    );

    match client.create_chat(args).await {
        Ok(res) => res
            .choices
            .first()
            .map(|choice| choice.message.content.clone())
            .unwrap_or_else(|| "OpenAI returned no response choices.".to_owned()),
        Err(error) => format!("OpenAI request failed: {error:?}"),
    }
}

pub async fn generate_image(
    ctx: Context,
    msg: Message,
    api_key: &str,
    prompt: &str,
) -> Result<(), String> {
    let client = Client::builder()
        .build()
        .map_err(|error| format!("Failed to build image HTTP client: {error}"))?;
    let model = env::var("OPENAI_IMAGE_MODEL").unwrap_or_else(|_| "gpt-image-2".to_owned());
    let url = "https://api.openai.com/v1/images/generations";

    let bot_msg = msg
        .reply(&ctx, "Select a size")
        .await
        .map_err(|error| format!("Failed to send size prompt: {error}"))?;

    let sizes = vec!["🟦", "↔", "↕️"];
    for emoji in &sizes {
        bot_msg
            .react(&ctx.http, ReactionType::Unicode(emoji.to_string()))
            .await
            .map_err(|error| format!("Failed to add size reactions: {error}"))?;
    }

    let mut selected_size = None;
    let selection_deadline = Instant::now() + Duration::from_secs(120);
    while selected_size.is_none() && Instant::now() < selection_deadline {
        for emoji in &sizes {
            let reaction_type = ReactionType::Unicode(emoji.to_string());
            if let Ok(users) = bot_msg
                .reaction_users(&ctx.http, reaction_type, None, None)
                .await
            {
                if users.iter().any(|user| user.id == msg.author.id) {
                    selected_size = match *emoji {
                        "🟦" => Some("1024x1024"),
                        "↔" => Some("1536x1024"),
                        "↕️" => Some("1024x1536"),
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

    let _ = bot_msg.delete(&ctx).await;
    let Some(size) = selected_size else {
        return Err("Image size selection timed out.".to_owned());
    };
    let size = size.to_string();

    let bot_msg_2 = msg
        .reply(&ctx, "Generating...")
        .await
        .map_err(|error| format!("Failed to send generation status: {error}"))?;

    let response = client
        .post(url)
        .bearer_auth(api_key)
        .json(&serde_json::json!({
            "prompt": prompt,
            "model": model,
            "n": 1,
            "quality": "high",
            "size": size
        }))
        .send()
        .await
        .map_err(|error| format!("Failed to send image request: {error}"))?;

    let status = response.status();
    let response_body = response
        .text()
        .await
        .map_err(|error| format!("Failed to read image response: {error}"))?;
    let _ = bot_msg_2.delete(&ctx).await;

    let json: Value = serde_json::from_str(&response_body)
        .map_err(|error| format!("Failed to parse image response JSON: {error}"))?;

    if !status.is_success() {
        let message = json["error"]["message"]
            .as_str()
            .unwrap_or("Image generation failed");
        return Err(message.to_owned());
    }

    if let Some(image_base64) = json["data"][0]["b64_json"].as_str() {
        let image_bytes = general_purpose::STANDARD
            .decode(image_base64)
            .map_err(|error| format!("Failed to decode generated image: {error}"))?;
        let attachment = CreateAttachment::bytes(image_bytes, "boss-bot-image.png");
        let message = CreateMessage::new()
            .content(format!("Generated with `{}`", model))
            .add_file(attachment);

        msg.channel_id
            .send_message(&ctx.http, message)
            .await
            .map_err(|error| format!("Failed to upload generated image: {error}"))?;
        return Ok(());
    }

    if let Some(image_url) = json["data"][0]["url"].as_str() {
        msg.reply(&ctx, image_url)
            .await
            .map_err(|error| format!("Failed to send generated image URL: {error}"))?;
        return Ok(());
    }

    Err("Image response did not include image data.".to_owned())
}

pub async fn say_queue(msg: Message, ctx: &Context, queue: VecDeque<Node>) {
    let mut name_str = String::from("🎵 **Queue** 🎵\n```markdown\n");
    let mut tracker = false;

    for (index, item) in queue.iter().enumerate() {
        tracker = true;
        let final_title = match get_video_title(&item.url).await {
            Ok(title) => title.trim().to_owned(),
            Err(why) => {
                println!("Error getting queued video title: {:?}", why);
                format!("{} (title unavailable)", item.url)
            }
        };
        name_str.push_str(&format!("{}: {}\n", index + 1, final_title));
    }
    name_str.push_str("```");

    if tracker {
        if let Err(why) = send_large_message(ctx, msg.channel_id, &name_str).await {
            println!("Error sending queue message: {:?}", why);
        }
    } else if let Err(why) = msg.channel_id.say(&ctx.http, "🪹 **Queue Empty** 🪹").await {
        println!("Error sending empty queue message: {:?}", why);
    }
}
