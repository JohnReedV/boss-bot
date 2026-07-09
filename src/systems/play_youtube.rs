use crate::resources::*;
use crate::systems::tracker;
use crate::utils::create_youtube_audio_input;
use serenity::{model::channel::Message, prelude::Context};
use std::{
    io,
    sync::{atomic::AtomicBool, Arc},
};

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

    let handler_lock = manager.get(guild_id).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotConnected,
            "not connected to a voice channel for playback",
        )
    })?;

    let node = {
        let mut queue = VIDEO_QUEUE.lock().await;
        queue.pop_front()
    };

    if let Some(node) = node {
        let source = create_youtube_audio_input(&node.url)?;
        let track_handle = {
            let mut handler = handler_lock.lock().await;
            handler.play_only_input(source.into())
        };

        track_handle.make_playable_async().await?;

        let ctx_clone = ctx.clone();
        tokio::spawn(async move {
            tracker(ctx_clone, skip, tracking_mutex, msg, node).await;
        });
    }

    Ok(())
}
