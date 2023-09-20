use crate::resources::*;
use crate::systems::tracker;
use serenity::{model::channel::Message, prelude::Context};
use songbird::input::ffmpeg_optioned;
use std::sync::{atomic::AtomicBool, Arc};
use tokio::process::Command as TokioCommand;

pub async fn play_youtube(
    ctx: &Context,
    msg: Message,
    skip: Arc<AtomicBool>,
    tracking_mutex: Arc<tokio::sync::Mutex<bool>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialization.")
        .clone();

    if let Some(handler_lock) = manager.get(guild_id) {
        let mut handler = handler_lock.lock().await;

        let mut queue = VIDEO_QUEUE.lock().await;
        if let Some(node) = queue.pop_front() {
            let output = TokioCommand::new("yt-dlp")
                .arg("-f")
                .arg("bestaudio")
                .arg("-g")
                .arg(&node.url)
                .output()
                .await?;
            let audio_url = String::from_utf8(output.stdout)?.trim().to_string();

            let source = ffmpeg_optioned(audio_url, &FFMPEG_OPTIONS, &AUDIO_OPTIONS).await?;
            let (track, _track_handle) = songbird::create_player(source);

            handler.play_only(track);

            let ctx_clone = ctx.clone();
            tokio::spawn(async move {
                tracker(ctx_clone, skip, tracking_mutex, msg, node).await;
            });
        }
    }

    Ok(())
}
