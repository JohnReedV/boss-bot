use crate::resources::*;
use crate::utils::*;
use crate::Handler;
use openai_rust::{chat::ChatArguments, chat::Message as OpenAiMessage, Client as OpenAiClient};
use serenity::{
    model::{channel::Message, prelude::GuildId},
    prelude::Context,
};
use songbird::Songbird;
use std::{
    collections::VecDeque,
    sync::{atomic::Ordering, Arc},
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

pub async fn chat_gpt(api_key: &str, prompt: &str) -> String {
    let client: OpenAiClient = OpenAiClient::new(api_key);
    let args: ChatArguments = ChatArguments::new(
        "gpt-3.5-turbo",
        vec![OpenAiMessage {
            role: "user".to_owned(),
            content: prompt.to_owned(),
        }],
    );

    let res = client.create_chat(args).await.unwrap();
    return format!("{}", res.choices[0].message.content.clone());
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
