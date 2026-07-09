use crate::resources::*;
use crate::systems::tracker;
use serenity::{model::channel::Message, prelude::Context};
use songbird::input::YoutubeDl;
use std::sync::{atomic::AtomicBool, Arc};

pub async fn play_youtube(
    ctx: &Context,
    msg: Message,
    skip: Arc<AtomicBool>,
    tracking_mutex: Arc<tokio::sync::Mutex<bool>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let guild_id = msg.guild_id.unwrap();

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialization.")
        .clone();

    if let Some(handler_lock) = manager.get(guild_id) {
        let mut handler = handler_lock.lock().await;

        let mut queue = VIDEO_QUEUE.lock().await;
        if let Some(node) = queue.pop_front() {
            let source = YoutubeDl::new(songbird_reqwest::Client::new(), node.url.clone());
            handler.play_only_input(source.into());

            let ctx_clone = ctx.clone();
            tokio::spawn(async move {
                tracker(ctx_clone, skip, tracking_mutex, msg, node).await;
            });
        }
    }

    Ok(())
}
