use lazy_static::lazy_static;
use openai_rust::{chat::ChatArguments, chat::Message as OpenAiMessage, Client as OpenAiClient};
use regex::Regex;
use serenity::{
    async_trait,
    client::{Client, EventHandler},
    framework::StandardFramework,
    model::{
        channel::Message,
        gateway::GatewayIntents,
        prelude::{ChannelId, GuildId},
    },
    prelude::*,
};
use songbird::{input::ffmpeg_optioned, SerenityInit, Songbird};
use std::{
    collections::VecDeque,
    env,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{process::Command as TokioCommand, sync::Mutex, time::sleep};

lazy_static! {
    #[derive(Debug)]
    static ref VIDEO_QUEUE: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
}

pub struct SongbirdKey;

impl TypeMapKey for SongbirdKey {
    type Value = Arc<Songbird>;
}

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

    let _ = client
        .start()
        .await
        .map_err(|why| println!("Client ended: {:?}", why));
}

struct Handler {
    pub playing: Arc<Mutex<bool>>,
    pub skip: Arc<AtomicBool>,
}

impl Default for Handler {
    fn default() -> Self {
        Handler {
            playing: Arc::new(Mutex::new(false)),
            skip: Arc::new(AtomicBool::new(false)),
        }
    }
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

                    let video_title = get_video_title(&url).await.unwrap();
                    let clean_video_title = video_title.replace("\n", "");
                    let new_content = format!("Added to queue: ```{}```", clean_video_title);
                    msg.channel_id.say(&ctx.http, new_content).await.unwrap();

                    {
                        let mut queue = VIDEO_QUEUE.lock().await;
                        queue.push_back(url.clone());
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
                                    {
                                        let queue = VIDEO_QUEUE.lock().await;
                                        should_continue = queue.front().is_some();
                                    }

                                    if should_continue {
                                        match self.playing.try_lock() {
                                            Ok(mut playing) => {
                                                if !*playing {
                                                    *playing = true;
                                                    let ctx_clone = ctx.clone();
                                                    let msg_clone = msg.clone();
                                                    tokio::spawn(async move {
                                                        play_youtube(&ctx_clone, msg_clone.clone())
                                                            .await
                                                            .unwrap();
                                                    });
                                                    let duration: Duration =
                                                        get_video_duration(&url).await.unwrap();
                                                    tokio::select! {
                                                        _ = sleep(duration) => {
                                                            *playing = false;
                                                        }
                                                        _ = async {
                                                            loop {
                                                                tokio::time::sleep(Duration::from_millis(100)).await;
                                                                if self.skip.load(Ordering::SeqCst) {
                                                                    self.skip.store(false, Ordering::SeqCst);
                                                                    *playing = false;
                                                                    break;
                                                                }
                                                            }
                                                        } => {
                                                            *playing = false;
                                                        }
                                                    }
                                                }
                                            }
                                            Err(_) => {}
                                        }
                                    } else {
                                        break;
                                    }
                                }
                            }
                        }
                        None => {
                            let _ = msg.reply(&ctx, "Join voice noob").await;
                        }
                    }
                }
                Err(_) => {
                    let _ = msg.reply(&ctx, "Bad URL").await;
                }
            }
        } else if message.starts_with("! q") || message.starts_with("!q") {
            println!("Got message: {}", message);
            if let Err(why) = msg.delete(&ctx).await {
                println!("Error deleting message: {:?}", why);
            }

            let queue = get_video_queue().lock().await;
            let mut name_str = String::from("🎵 **Queue** 🎵\n```markdown\n");
            let mut tracker = false;

            for (index, item) in queue.iter().enumerate() {
                tracker = true;
                let title = get_video_title(&item).await.unwrap();
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
        } else if message.starts_with("! skip") || message.starts_with("!skip") {
            println!("Got message: {}", message);

            skip_current_song(guild_id, manager, self.clone())
                .await
                .unwrap();
        } else if message.starts_with("! leave") || message.starts_with("!leave") {
            println!("Got message: {}", message);

            if manager.get(guild_id).is_some() {
                manager.remove(guild_id).await.unwrap();
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
            let _ = msg.channel_id.say(&ctx.http, help_message).await;
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

            let _ = send_large_message(&ctx, msg.channel_id, &response)
                .await
                .expect("Expected to send_large_message");
        }
    }
}

async fn play_youtube(
    ctx: &Context,
    msg: Message,
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
        if let Some(url) = queue.pop_front() {
            let output = TokioCommand::new("yt-dlp")
                .arg("-f")
                .arg("bestaudio")
                .arg("-g")
                .arg(&url)
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

    app.skip.store(true, Ordering::SeqCst);
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

async fn send_large_message(
    ctx: &Context,
    channel_id: ChannelId,
    message: &str,
) -> serenity::Result<()> {
    let max_length = 1950;
    let mut start = 0;
    let mut end = std::cmp::min(max_length, message.len());

    while start < message.len() {
        let part = &message[start..end];
        channel_id.say(&ctx.http, part).await?;

        start = end;
        end = std::cmp::min(end + max_length, message.len());
    }

    Ok(())
}

fn extract_youtube_url(input: &str) -> Result<String, Box<dyn std::error::Error + Send>> {
    let start_index = input.find("https://www.youtube.com/watch?v=");
    match start_index {
        Some(start) => {
            let potential_url = &input[start..];
            if is_valid_youtube_url(potential_url) {
                return Ok(potential_url.to_string());
            }
            Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "No valid YouTube URL found",
            )))
        }
        None => Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "No valid YouTube URL found",
        ))),
    }
}

fn is_valid_youtube_url(url: &str) -> bool {
    let re = Regex::new(r"https?://(www\.)?youtube\.com/watch\?v=[a-zA-Z0-9_-]+").unwrap();
    return re.is_match(url);
}

pub fn get_video_queue() -> &'static Mutex<VecDeque<String>> {
    &VIDEO_QUEUE
}

async fn get_video_title(video_url: &String) -> Result<String, std::io::Error> {
    let output = Command::new("yt-dlp")
        .arg("--get-title")
        .arg(video_url)
        .output()?;

    if output.status.success() {
        return Ok(String::from_utf8(output.stdout).unwrap());
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "yt-dlp failed to get video duration",
        ));
    }
}

async fn get_video_duration(video_url: &str) -> std::io::Result<Duration> {
    let output = Command::new("yt-dlp")
        .arg("--get-duration")
        .arg(video_url)
        .output()?;

    if output.status.success() {
        let duration_str = String::from_utf8(output.stdout).unwrap();
        let duration_parts: Vec<&str> = duration_str.trim().split(":").collect();
        let duration = match duration_parts.len() {
            3 => {
                let hrs: u64 = duration_parts[0].parse().unwrap();
                let mins: u64 = duration_parts[1].parse().unwrap();
                let secs: u64 = duration_parts[2].parse().unwrap();
                Duration::from_secs(hrs * 3600 + mins * 60 + secs)
            }
            2 => {
                let mins: u64 = duration_parts[0].parse().unwrap();
                let secs: u64 = duration_parts[1].parse().unwrap();
                Duration::from_secs(mins * 60 + secs)
            }
            1 => {
                let secs: u64 = duration_parts[0].parse().unwrap();
                Duration::from_secs(secs)
            }
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Couldn't parse duration",
                ))
            }
        };

        Ok(duration)
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "yt-dlp failed to get video duration",
        ));
    }
}
