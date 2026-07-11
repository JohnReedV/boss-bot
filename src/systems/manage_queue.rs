use crate::resources::*;
use crate::systems::play_youtube;
use crate::utils::*;
use crate::Handler;
use serenity::{
    model::{channel::Message, prelude::GuildId},
    prelude::Context,
};
use songbird::Songbird;
use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use tokio::time::sleep;

pub async fn manage_queue(
    message: &str,
    msg: Message,
    guild_id: GuildId,
    ctx: &Context,
    manager: Arc<Songbird>,
    app: &Handler,
) {
    match extract_youtube_url(message) {
        Ok(url) => {
            if let Err(why) = msg.delete(ctx).await {
                println!("Error deleting message: {:?}", why);
            }

            let is_looping = *app.looping.lock().await;
            if is_looping {
                let _ = msg.channel_id.say(&ctx.http, "You loopin rn").await;
                return;
            }

            let channel_id = {
                let Some(guild) = ctx.cache.guild(guild_id) else {
                    let _ = msg
                        .reply(
                            ctx,
                            "I couldn't find this server in cache. Try again in a moment.",
                        )
                        .await;
                    return;
                };

                guild
                    .voice_states
                    .get(&msg.author.id)
                    .and_then(|voice_state| voice_state.channel_id)
            };

            let Some(channel) = channel_id else {
                let _ = msg.reply(ctx, "Join voice noob").await;
                return;
            };

            let duration: Duration = match get_video_duration(url).await {
                Ok(duration) => duration,
                Err(why) => {
                    println!("Error getting video duration: {:?}", why);
                    let _ = msg
                        .channel_id
                        .say(
                            &ctx.http,
                            format!("Couldn't inspect that YouTube URL: {}", why),
                        )
                        .await;
                    return;
                }
            };

            let should_drive_queue = {
                let mut queue = VIDEO_QUEUE.lock().await;
                queue.push_back(Node::from(url.to_string(), duration));
                let mut playing = app.playing.lock().await;
                if *playing {
                    false
                } else {
                    *playing = true;
                    true
                }
            };

            if !should_drive_queue {
                return;
            }

            if let Err(why) = manager.join(guild_id, channel).await {
                println!("Failed to join voice: {:?}", why);
                {
                    let mut queue = VIDEO_QUEUE.lock().await;
                    queue.clear();
                }
                {
                    let mut playing = app.playing.lock().await;
                    *playing = false;
                }
                let _ = msg
                    .channel_id
                    .say(
                        &ctx.http,
                        format!("Couldn't join your voice channel: {}", why),
                    )
                    .await;
                return;
            }

            loop {
                let duration = {
                    let queue = VIDEO_QUEUE.lock().await;
                    queue.front().map(|node| node.duration)
                };

                let Some(the_duration) = duration else {
                    let queue_has_item = {
                        let queue = VIDEO_QUEUE.lock().await;
                        queue.front().is_some()
                    };

                    if queue_has_item {
                        continue;
                    }

                    let mut playing = app.playing.lock().await;
                    *playing = false;
                    break;
                };

                let tracker_clone = app.tracking.clone();
                let skip_tracker_clone = app.skip_tracker.clone();
                let ctx_clone = ctx.clone();
                let msg_clone = msg.clone();

                if let Err(why) = play_youtube(
                    &ctx_clone,
                    msg_clone.clone(),
                    skip_tracker_clone,
                    tracker_clone,
                )
                .await
                {
                    println!("Error playing YouTube audio: {:?}", why);
                    let _ = msg
                        .channel_id
                        .say(&ctx.http, format!("Couldn't play that audio: {}", why))
                        .await;
                    continue;
                }

                tokio::select! {
                    _ = sleep(the_duration + Duration::from_secs(1)) => {}
                    _ = async {
                        loop {
                            sleep(Duration::from_millis(100)).await;
                            if app.skip_player.load(Ordering::SeqCst) {
                                app.skip_player.store(false, Ordering::SeqCst);
                                break;
                            }
                        }
                    } => {
                        let is_tracking = *app.tracking.lock().await;
                        if is_tracking {
                            app.skip_tracker.store(true, Ordering::SeqCst);
                            wait_for_tracker_to_stop(app).await;
                        }
                    }
                }
            }
        }
        Err(_) => {
            let _ = msg.reply(ctx, "Bad URL").await;
        }
    }
}

async fn wait_for_tracker_to_stop(app: &Handler) {
    for _ in 0..100 {
        if !*app.tracking.lock().await {
            return;
        }
        sleep(Duration::from_millis(50)).await;
    }
}
