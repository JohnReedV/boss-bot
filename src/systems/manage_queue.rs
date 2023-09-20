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
            if let Err(why) = msg.delete(&ctx).await {
                println!("Error deleting message: {:?}", why);
            }
            {
                let looping_lock = app.looping.try_lock();
                match looping_lock {
                    Ok(lock) => {
                        if *lock {
                            msg.channel_id
                                .say(&ctx.http, "You loopin rn")
                                .await
                                .unwrap();

                            return;
                        }
                    }
                    Err(_) => {}
                }
            }
            let duration: Duration = get_video_duration(&url).await.unwrap();

            {
                let mut queue = VIDEO_QUEUE.lock().await;
                queue.push_back(Node::from(url.to_string(), duration));
            }

            let guild = ctx.cache.guild(guild_id).unwrap();
            let channel_id = guild
                .voice_states
                .get(&msg.author.id)
                .and_then(|voice_state| voice_state.channel_id);

            match channel_id {
                Some(channel) => {
                    let (_handler_lock, success) = manager.join(guild_id, channel).await;
                    if success.is_ok() {
                        loop {
                            let should_continue: bool;
                            let the_duration: Duration;
                            {
                                let queue = VIDEO_QUEUE.lock().await;
                                should_continue = queue.front().is_some();

                                the_duration = match queue.front() {
                                    Some(node) => node.duration,
                                    None => return,
                                }
                            }

                            if should_continue {
                                let mut unlock: bool = false;
                                {
                                    let lock = app.playing.try_lock();
                                    if lock.is_ok() {
                                        let mut playing = lock.unwrap();
                                        unlock = !*playing;
                                        if unlock {
                                            *playing = true;
                                        }
                                    }
                                }

                                if unlock {
                                    let tracker_clone = app.tracking.clone();
                                    let skip_tracker_clone = app.skip_tracker.clone();
                                    let ctx_clone = ctx.clone();
                                    let msg_clone = msg.clone();

                                    tokio::spawn(async move {
                                        play_youtube(
                                            &ctx_clone,
                                            msg_clone.clone(),
                                            skip_tracker_clone,
                                            tracker_clone,
                                        )
                                        .await
                                        .unwrap();
                                    });

                                    tokio::select! {
                                        _ = sleep(the_duration + Duration::from_secs(1)) => {
                                            {
                                                let mut playing_lock = app.playing.lock().await;
                                                *playing_lock = false;
                                            }
                                        }
                                        _ = async {
                                            loop {
                                                sleep(Duration::from_millis(100)).await;
                                                if app.skip_player.load(Ordering::SeqCst) {
                                                    app.skip_player.store(false, Ordering::SeqCst);
                                                    break;
                                                }
                                            }
                                        } => {
                                            {
                                                let mut playing_lock = app.playing.lock().await;
                                                *playing_lock = false;
                                            }
                                            {
                                                if *app.tracking.lock().await {
                                                    app.skip_tracker.store(true, Ordering::SeqCst);
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                break;
                            }
                        }
                    }
                }
                None => {
                    msg.reply(&ctx, "Join voice noob").await.unwrap();
                }
            }
        }
        Err(_) => {
            msg.reply(&ctx, "Bad URL").await.unwrap();
        }
    }
}
