use crate::resources::*;
use crate::systems::tracker;
use crate::utils::*;
use crate::Handler;
use serenity::{model::channel::Message, prelude::Context};
use std::{sync::atomic::Ordering, time::Duration};
use tokio::time::sleep;

pub async fn loop_song(
    app: &Handler,
    full_message: &str,
    msg: Message,
    ctx: &Context,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let url = match extract_youtube_url(full_message) {
        Ok(url) => url,
        Err(_) => {
            let _ = msg.reply(ctx, "Bad URL").await;
            return Ok(());
        }
    };

    let count = match parse_loop_count(full_message, url) {
        Ok(count) => count,
        Err(message) => {
            let _ = msg.channel_id.say(&ctx.http, message).await;
            return Ok(());
        }
    };

    let already_looping = {
        let mut looping_lock = app.looping.lock().await;
        if *looping_lock {
            true
        } else {
            *looping_lock = true;
            false
        }
    };

    if already_looping {
        let _ = msg.channel_id.say(&ctx.http, "You loopin rn").await;
        return Ok(());
    }

    let result = run_loop_song(app, url, count, msg, ctx).await;

    {
        let mut looping_lock = app.looping.lock().await;
        *looping_lock = false;
    }

    result
}

async fn run_loop_song(
    app: &Handler,
    url: &str,
    count: usize,
    msg: Message,
    ctx: &Context,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let is_playing = *app.playing.lock().await;
    if is_playing {
        let _ = msg
            .channel_id
            .say(&ctx.http, "not loopin til queue done")
            .await;
        println!("not looping while music queue running");
        return Ok(());
    }

    let Some(guild_id) = msg.guild_id else {
        let _ = msg
            .reply(ctx, "I only handle loop commands inside a server.")
            .await;
        return Ok(());
    };

    let channel_id = {
        let Some(guild) = ctx.cache.guild(guild_id) else {
            let _ = msg
                .reply(
                    ctx,
                    "I couldn't find this server in cache. Try again in a moment.",
                )
                .await;
            return Ok(());
        };

        guild
            .voice_states
            .get(&msg.author.id)
            .and_then(|voice_state| voice_state.channel_id)
    };

    let Some(channel_id) = channel_id else {
        let _ = msg.reply(ctx, "Join voice noob").await;
        return Ok(());
    };

    let duration = match get_video_duration(url).await {
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
            return Ok(());
        }
    };

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialization.")
        .clone();
    if let Err(why) = manager.join(guild_id, channel_id).await {
        println!("Failed to join voice: {:?}", why);
        let _ = msg
            .channel_id
            .say(
                &ctx.http,
                format!("Couldn't join your voice channel: {}", why),
            )
            .await;
        return Ok(());
    }

    let Some(handler_lock) = manager.get(guild_id) else {
        let _ = msg
            .channel_id
            .say(&ctx.http, "Failed to prepare voice playback.")
            .await;
        return Ok(());
    };

    let _ = msg
        .channel_id
        .say(
            &ctx.http,
            format!("looping {} times for my king {}", count, msg.author.clone()),
        )
        .await;

    let mut iterations = 0;
    loop {
        let source = match create_youtube_audio_input(url) {
            Ok(source) => source,
            Err(why) => return Err(Box::new(why)),
        };

        let ctx_clone = ctx.clone();
        let tracker_clone = app.tracking.clone();
        let skip_tracker_clone = app.skip_tracker.clone();
        let msg_clone = msg.clone();
        let node = Node::from(url.to_string(), duration);

        let track_handle = {
            let mut handler = handler_lock.lock().await;
            handler.play_only_input(source)
        };

        if let Err(why) = track_handle.make_playable_async().await {
            return Err(Box::new(why));
        }

        {
            let mut current_song = app.current_song.lock().await;
            *current_song = Some(node.clone());
        }

        tokio::spawn(async move {
            tracker(
                ctx_clone,
                skip_tracker_clone,
                tracker_clone,
                msg_clone,
                node,
            )
            .await;
        });

        tokio::select! {
            _ = sleep(duration + Duration::from_secs(1)) => {
                iterations += 1;
                if iterations >= count {
                    let mut current_song = app.current_song.lock().await;
                    *current_song = None;
                    return Ok(());
                }
            }
            _ = wait_for_loop_skip(app) => {
                let is_tracking = *app.tracking.lock().await;
                if is_tracking {
                    app.skip_tracker.store(true, Ordering::SeqCst);
                    wait_for_tracker_to_stop(app).await;
                }
                let mut current_song = app.current_song.lock().await;
                *current_song = None;
                return Ok(());
            }
        }

        {
            let mut current_song = app.current_song.lock().await;
            *current_song = None;
        }
    }
}

fn parse_loop_count(message: &str, url: &str) -> Result<usize, &'static str> {
    let message_without_url = message.replacen(url, "", 1);
    let Some(loop_count_message) = message_without_url.split_whitespace().next() else {
        return Err("No loop count specified. Expected format `!loop <count> <url>`");
    };

    let count = loop_count_message
        .parse::<usize>()
        .map_err(|_| "Loop count must be a positive whole number.")?;

    if count == 0 {
        return Err("Loop count must be greater than 0.");
    }

    Ok(count)
}

async fn wait_for_loop_skip(app: &Handler) {
    loop {
        sleep(Duration::from_millis(100)).await;
        if app.skip_loop.load(Ordering::SeqCst) {
            app.skip_loop.store(false, Ordering::SeqCst);
            break;
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

#[cfg(test)]
mod tests {
    use super::parse_loop_count;

    const URL: &str = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";

    #[test]
    fn parses_positive_loop_count() {
        assert_eq!(parse_loop_count(&format!("3 {URL}"), URL).unwrap(), 3);
    }

    #[test]
    fn rejects_zero_negative_and_missing_loop_counts() {
        assert!(parse_loop_count(&format!("0 {URL}"), URL).is_err());
        assert!(parse_loop_count(&format!("-1 {URL}"), URL).is_err());
        assert!(parse_loop_count(URL, URL).is_err());
    }
}
