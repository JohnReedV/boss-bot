use openai_rust::{chat::ChatArguments, chat::Message as OpenAiMessage, Client as OpenAiClient};
use serenity::{
    async_trait,
    client::{Client, EventHandler},
    framework::StandardFramework,
    model::{channel::Message, gateway::GatewayIntents, prelude::GuildId},
    prelude::Context,
};
use songbird::{input::ffmpeg_optioned, SerenityInit, Songbird};
use std::{
    collections::VecDeque,
    env,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    process::Command as TokioCommand,
    time::{sleep, Instant},
};
pub mod resources;
use resources::*;
pub mod utils;
use utils::*;

#[tokio::main]
async fn main() {
    dotenv::dotenv().expect("Failed to load .env file");
    let token = env::var("DISCORD_KEY").expect("Expected a token in the environment");

    let framework = StandardFramework::new().configure(|c| c.prefix("~"));
    let mut client = Client::builder(&token, GatewayIntents::all())
        .event_handler(Handler::default())
        .framework(framework)
        .register_songbird()
        .await
        .expect("Err creating client");

    let songbird: Arc<Songbird> = Songbird::serenity();
    {
        let mut data = client.data.write().await;
        data.insert::<SongbirdKey>(songbird.clone());
    }

    client
        .start()
        .await
        .map_err(|why| println!("Client ended: {:?}", why))
        .unwrap();
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        let mut message: &str = msg.content.trim();
        if msg.author.bot {
            return;
        }

        let guild_id = msg.guild_id.unwrap();
        let manager = songbird::get(&ctx)
            .await
            .expect("Songbird Voice client placed in at initialization.")
            .clone();

        if message.starts_with("! https://") || message.starts_with("!https://") {
            message = message.split_at(1).1;

            println!("Got yt message: {}", message);
            match extract_youtube_url(message) {
                Ok(url) => {
                    if let Err(why) = msg.delete(&ctx).await {
                        println!("Error deleting message: {:?}", why);
                    }
                    let duration: Duration = get_video_duration(&url).await.unwrap();

                    {
                        let mut queue = VIDEO_QUEUE.lock().await;
                        queue.push_back(Node::from(url.clone(), duration));
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
                                            let lock = self.playing.try_lock();
                                            if lock.is_ok() {
                                                let mut playing = lock.unwrap();
                                                unlock = !*playing;
                                                if unlock {
                                                    *playing = true;
                                                }
                                            }
                                        }

                                        if unlock {
                                            let tracker_clone = self.tracking.clone();
                                            let skip_tracker_clone = self.skip_tracker.clone();
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
                                                _ = sleep(the_duration) => {
                                                    {
                                                        let mut playing_lock = self.playing.lock().await;
                                                        *playing_lock = false;

                                                        let mut tracking_lock = self.tracking.lock().await;
                                                        *tracking_lock = false;
                                                    }
                                                }
                                                _ = async {
                                                    loop {
                                                        tokio::time::sleep(Duration::from_millis(100)).await;
                                                        if self.skip_player.load(Ordering::SeqCst) {
                                                            self.skip_player.store(false, Ordering::SeqCst);
                                                            break;
                                                        }
                                                    }
                                                } => {
                                                    {
                                                        let mut playing_lock = self.playing.lock().await;
                                                        *playing_lock = false;

                                                        let mut tracking_lock = self.tracking.lock().await;
                                                        *tracking_lock = false;
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
        } else if message.starts_with("! q") || message.starts_with("!q") {
            println!("Got message: {}", message);
            if let Err(why) = msg.delete(&ctx).await {
                println!("Error deleting message: {:?}", why);
            }

            let queue = get_video_queue().lock().await;
            say_queue(msg.clone(), &ctx, queue).await;
        } else if message.starts_with("! skip") || message.starts_with("!skip") {
            println!("Got message: {}", message);

            skip_current_song(guild_id, manager, self.clone())
                .await
                .unwrap();
        } else if message.starts_with("! leave") || message.starts_with("!leave") {
            println!("Got message: {}", message);

            if manager.get(guild_id).is_some() {
                manager.remove(guild_id).await.unwrap();
                skip_current_song(guild_id, manager, self.clone())
                    .await
                    .unwrap();
            }
            {
                let mut queue = VIDEO_QUEUE.lock().await;
                queue.clear();
            }
        } else if message.starts_with("! help") || message.starts_with("!help") {
            println!("Got message: {}", message);

            if let Err(why) = msg.delete(&ctx).await {
                println!("Error deleting message: {:?}", why);
            }

            let help_message = "💅🏻 **Woman Commands** ☕\n\
            ```markdown\n\
            1. !https://<URL>  -- Add a YouTube video to the queue\n\
            2. !q              -- Display the current audio queue\n\
            3. !skip           -- Skip the currently playing song\n\
            4. !leave          -- Leave the voice channel and clear the queue\n\
            5. !help           -- Displays this page\n\
            6. !               -- Everything proceeding from \"!\" is a GPT prompt\n\
            ```";
            msg.channel_id.say(&ctx.http, help_message).await.unwrap();
        } else if message.starts_with("!") {
            message = message.split_at(2).1;
            println!("Got message: {}", message);

            let api_key: String = env::var("OPENAI_KEY").expect("Expected OPENAI_KEY to be set");
            let prompt: String = message.to_string();

            let response: String = tokio::task::spawn_blocking(move || {
                let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(chat_gpt(&api_key, &prompt))
            })
            .await
            .unwrap();

            send_large_message(&ctx, msg.channel_id, &response)
                .await
                .expect("Expected to send_large_message");
        }
    }
}

async fn play_youtube(
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

            let ffmpeg_options: [&str; 6] = [
                "-reconnect",
                "1",
                "-reconnect_streamed",
                "1",
                "-reconnect_delay_max",
                "5",
            ];

            let audio_options: [&str; 9] = [
                "-f",
                "s16le",
                "-ac",
                "2",
                "-ar",
                "48000",
                "-acodec",
                "pcm_f32le",
                "-",
            ];

            let source = ffmpeg_optioned(audio_url, &ffmpeg_options, &audio_options).await?;
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

async fn tracker(
    ctx: Context,
    skip: Arc<AtomicBool>,
    tracking_mutex: Arc<tokio::sync::Mutex<bool>>,
    msg: Message,
    node: Node,
) {
    let mut unlock: bool = false;
    while !unlock {
        {
            let lock = tracking_mutex.try_lock();
            if lock.is_ok() {
                let mut tracking = lock.unwrap();
                unlock = !*tracking;
                if unlock {
                    *tracking = true;
                }
            }
        }
    }

    println!("Entering");
    let mut first_update = true;
    let mut current_time = 0;
    let start_time = Instant::now();
    let mut next_tick = start_time + Duration::from_secs(1);

    let url = node.url;
    let duration = node.duration;

    let video_title = get_video_title(&url).await.unwrap();
    let clean_video_title = video_title.replace("\n", "");
    let new_content = format!("Playing: ```{}```", clean_video_title);
    let mut created_message = msg
        .channel_id
        .say(&ctx.http, new_content.clone())
        .await
        .unwrap();

    let content = created_message.content.replace("Playing:", "");

    while current_time < duration.as_secs() {
        let now = Instant::now();
        if now >= next_tick {
            current_time += 1;
            let duration_str = format!(
                "{}:{:02} / {}:{:02}",
                current_time / 60,
                current_time % 60,
                duration.as_secs() / 60,
                duration.as_secs() % 60
            );

            let progress = (current_time as f64 / duration.as_secs() as f64) * 25.0;
            let progress_bar: String = std::iter::repeat("█").take(progress as usize).collect();
            let empty_space: String = std::iter::repeat("░")
                .take(25 - progress as usize)
                .collect();

            let combined_field = format!("{}\n{}{}", duration_str, progress_bar, empty_space);

            created_message
                .edit(&ctx.http, |m| {
                    if first_update {
                        m.content(" ");
                        first_update = false;
                    }
                    m.embed(|e| {
                        e.title("Now Playing").description(&content).field(
                            "Progress",
                            &combined_field,
                            false,
                        )
                    })
                })
                .await
                .unwrap();

            next_tick += Duration::from_secs(1);
        }

        if skip.load(Ordering::SeqCst) {
            skip.store(false, Ordering::SeqCst);
            break;
        }
        {
            let tracking = tracking_mutex.lock().await;
            if *tracking == false {
                break;
            }
        }
    }
    created_message.delete(&ctx.http).await.unwrap();
    {
        let mut tracking = tracking_mutex.lock().await;
        *tracking = false;
    }
}

async fn skip_current_song(
    guild_id: GuildId,
    manager: Arc<Songbird>,
    app: &Handler,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(handler_lock) = manager.get(guild_id) {
        let mut handler = handler_lock.lock().await;
        handler.stop();
    }

    app.skip_player.store(true, Ordering::SeqCst);
    app.skip_tracker.store(true, Ordering::SeqCst);
    Ok(())
}

async fn chat_gpt(api_key: &str, prompt: &str) -> String {
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

async fn say_queue(
    msg: Message,
    ctx: &Context,
    queue: tokio::sync::MutexGuard<'_, VecDeque<Node>>,
) {
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
