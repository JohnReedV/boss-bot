use crate::resources::*;
use crate::systems::tracker;
use crate::utils::*;
use crate::Handler;
use serenity::{model::channel::Message, prelude::Context};
use songbird::input::ffmpeg_optioned;
use std::{sync::atomic::Ordering, time::Duration};
use tokio::{process::Command as TokioCommand, time::sleep};

pub async fn loop_song(
    app: &Handler,
    full_message: &str,
    msg: Message,
    ctx: &Context,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match extract_youtube_url(full_message) {
        Ok(url) => {
            let message: String = full_message.replace(url, "");

            if let Some(cap) = RE.captures_iter(&message).next() {
                {
                    let playing_lock = app.playing.try_lock();
                    match playing_lock {
                        Ok(lock) => {
                            if *lock {
                                msg.channel_id
                                    .say(&ctx.http, "not loopin til queue done")
                                    .await
                                    .unwrap();

                                println!("not looping while music queue running");
                                return Ok(());
                            }
                        }
                        Err(_) => {}
                    }
                }

                let loop_count_message = &cap[0];
                let count = match loop_count_message.parse::<usize>() {
                    Ok(i) => i,
                    Err(_) => usize::MAX,
                };

                let guild = msg.guild(&ctx.cache).unwrap();
                let guild_id = guild.id;

                let channel_id = guild
                    .voice_states
                    .get(&msg.author.id)
                    .and_then(|voice_state| voice_state.channel_id)
                    .unwrap();

                let manager = songbird::get(ctx)
                    .await
                    .expect("Songbird Voice client placed in at initialization.")
                    .clone();
                let (_handler_lock, _success) = manager.join(guild_id, channel_id).await;

                if let Some(handler_lock) = manager.get(guild_id) {
                    let duration = get_video_duration(url).await.unwrap();
                    msg.channel_id
                        .say(
                            &ctx.http,
                            format!("looping {} times for my king {}", count, msg.author.clone()),
                        )
                        .await
                        .unwrap();

                    {
                        let mut looping_lock = app.looping.lock().await;
                        *looping_lock = true;
                    }

                    let mut iterations = 0;
                    loop {
                        let output = TokioCommand::new("yt-dlp")
                            .arg("-f")
                            .arg("bestaudio")
                            .arg("-g")
                            .arg(&url)
                            .output()
                            .await?;
                        let audio_url = String::from_utf8(output.stdout)?.trim().to_string();

                        let source =
                            ffmpeg_optioned(audio_url, &FFMPEG_OPTIONS, &AUDIO_OPTIONS).await?;

                        let (track, _track_handle) = songbird::create_player(source);

                        let ctx_clone = ctx.clone();
                        let tracker_clone = app.tracking.clone();
                        let skip_tracker_clone = app.skip_tracker.clone();
                        let msg_clone = msg.clone();
                        let node = Node::from(url.to_string(), duration);

                        {
                            let mut handler = handler_lock.lock().await;
                            handler.play_only(track);
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
                                    let mut looping_lock = app.looping.lock().await;
                                    *looping_lock = false;
                                    drop(looping_lock);
                                    break;
                                }
                            }
                            _ = async {
                                loop {
                                    sleep(Duration::from_millis(100)).await;
                                    if app.skip_loop.load(Ordering::SeqCst) {
                                        app.skip_loop.store(false, Ordering::SeqCst);
                                        break;
                                    }
                                }
                            } => {
                                let mut looping_lock = app.looping.lock().await;
                                *looping_lock = false;
                                drop(looping_lock);
                                break;
                            }
                        }
                    }
                } else {
                    println!("Failed to join voice");
                }
            } else {
                msg.channel_id
                    .say(
                        &ctx.http,
                        "No loop count specified. Expected format '!loop <count> <url>'",
                    )
                    .await
                    .unwrap();
            }
        }
        Err(_) => {
            msg.reply(ctx, "Bad URL").await.unwrap();
        }
    }
    Ok(())
}
