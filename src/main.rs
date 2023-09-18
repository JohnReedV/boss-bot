use openai_rust::{chat::ChatArguments, chat::Message as OpenAiMessage, Client as OpenAiClient};
use regex::Regex;
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
        println!("Got message: {}", message);

        let guild_id = msg.guild_id.unwrap();
        let manager = songbird::get(&ctx)
            .await
            .expect("Songbird Voice client placed in at initialization.")
            .clone();

        if message.starts_with("! https://") || message.starts_with("!https://") {
            message = message.split_at(1).1;

            manage_queue(message, msg.clone(), guild_id, &ctx, manager, self.clone()).await;
        } else if message.starts_with("! q") || message.starts_with("!q") {
            msg.delete(&ctx).await.unwrap();
            let queue = get_video_queue().lock().await;

            say_queue(msg.clone(), &ctx, queue).await;
        } else if message.starts_with("! skip") || message.starts_with("!skip") {
            skip_current_song(guild_id, manager, self.clone())
                .await
                .unwrap();
        } else if message.starts_with("! leave") || message.starts_with("!leave") {
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
            msg.delete(&ctx).await.unwrap();
            msg.channel_id.say(&ctx.http, HELP_MESSAGE).await.unwrap();
        } else if message.starts_with("! loop ") || message.starts_with("!loop ") {
            msg.delete(&ctx).await.unwrap();
            loop_song(self.clone(), message, msg.clone(), &ctx)
                .await
                .unwrap();
        } else if message.starts_with("!") {
            message = message.split_at(2).1;

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

async fn manage_queue(
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

                                                if *app.tracking.lock().await {
                                                    app.skip_tracker.store(true, Ordering::SeqCst);
                                                }
                                            }
                                        }
                                        _ = async {
                                            loop {
                                                tokio::time::sleep(Duration::from_millis(100)).await;
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

async fn tracker(
    ctx: Context,
    skip: Arc<AtomicBool>,
    tracking_mutex: Arc<tokio::sync::Mutex<bool>>,
    msg: Message,
    node: Node,
) {
    {
        let mut tracking = tracking_mutex.lock().await;
        *tracking = true;
    }

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
    }
    created_message.delete(&ctx.http).await.unwrap();
    {
        let mut tracking = tracking_mutex.lock().await;
        *tracking = false;
    }
}

async fn loop_song(
    app: &Handler,
    full_message: &str,
    msg: Message,
    ctx: &Context,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match extract_youtube_url(full_message) {
        Ok(url) => {
            let message: String = full_message.replace(url, "");

            if let Some(cap) = RE.captures_iter(&message).next() {
                skip_all_enabled(app).await;

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
                            _ = sleep(duration + Duration::from_secs(1)) => {}
                            _ = async {
                                loop {
                                    sleep(Duration::from_millis(100)).await;
                                    if app.skip_loop.load(Ordering::SeqCst) {
                                        app.skip_loop.store(false, Ordering::SeqCst);
                                        break;
                                    }
                                }
                            } => { break;}
                        }
                        iterations += 1;
                        if iterations >= count {
                            break;
                        }
                    }
                    {
                        let mut looping_lock = app.looping.lock().await;
                        *looping_lock = false;
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
    app.skip_loop.store(true, Ordering::SeqCst);
    Ok(())
}

async fn skip_all_enabled(app: &Handler) {
    {
        let mut queue = VIDEO_QUEUE.lock().await;
        queue.clear();
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
