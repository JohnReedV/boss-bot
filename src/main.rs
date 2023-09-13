use lazy_static::lazy_static;
use openai_rust::{chat::ChatArguments, chat::Message as OpenAiMessage, Client as OpenAiClient};
use regex::Regex;
use serenity::{
    async_trait,
    client::{Client, EventHandler},
    framework::StandardFramework,
    model::{channel::Message, gateway::GatewayIntents, prelude::ChannelId},
    prelude::*,
};
use songbird::{SerenityInit, Songbird};
use std::{collections::VecDeque, env, process::Command, sync::Arc, time::Duration};
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
}

impl Default for Handler {
    fn default() -> Self {
        Handler {
            playing: Arc::new(Mutex::new(false)),
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

        if message.starts_with("ye https://") || message.starts_with("Ye https://") {
            message = message.split_at(3).1;

            println!("Got yt message: {}", message);
            match extract_youtube_url(message) {
                Ok(url) => {
                    {
                        let mut queue = VIDEO_QUEUE.lock().await;
                        queue.push_back(url.clone());
                    }
                    let guild_id = msg.guild_id.unwrap();
                    let guild = ctx.cache.guild(guild_id).unwrap();
                    let channel_id = guild
                        .voice_states
                        .get(&msg.author.id)
                        .and_then(|voice_state| voice_state.channel_id);

                    match channel_id {
                        Some(channel) => {
                            let manager = songbird::get(&ctx)
                                .await
                                .expect("Songbird Voice client placed in at initialization.")
                                .clone();

                            let (_handler_lock, success) = manager.join(guild_id, channel).await;
                            if success.is_ok() {
                                loop {
                                    let should_continue: bool;
                                    {
                                        let queue = VIDEO_QUEUE.lock().await;
                                        should_continue = queue.front().is_some();
                                    }

                                    if should_continue {
                                        let mut playing: tokio::sync::MutexGuard<'_, bool> =
                                            self.playing.lock().await;
                                        if !*playing {
                                            *playing = true;
                                            let _ = play_youtube(&ctx, msg.clone()).await;
                                            let _ = sleep_for_video_duration(&url).await;
                                            *playing = false;
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
        } else if message.starts_with("ye q") || message.starts_with("Ye q") {
            let queue = get_video_queue().lock().await;
            let mut name_vec: VecDeque<String> = VecDeque::new();

            for item in &*queue {
                let title = get_video_title(&item).await.unwrap();
                let final_title = title.trim().to_string();
                name_vec.push_back(final_title);
            }

            msg.channel_id
                .say(&ctx.http, format!("{:#?}", name_vec))
                .await
                .unwrap();
        } else if message.starts_with("ye skip") || message.starts_with("Ye skip") {
            let _ = skip_current_song(&ctx, msg, self.playing.clone()).await;
        } else if message.starts_with("Ye") || message.starts_with("ye") {
            message = message.split_at(3).1;
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

            let source = match songbird::ffmpeg(audio_url).await {
                Ok(source) => source,
                Err(why) => {
                    println!("Err starting source: {:?}", why);
                    let _ = msg.channel_id.say(&ctx.http, "Can't play that one").await;
                    return Ok(());
                }
            };

            handler.play_only_source(source);
        }
    }

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

async fn sleep_for_video_duration(video_url: &str) -> std::io::Result<()> {
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

        sleep(duration).await;
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "yt-dlp failed to get video duration",
        ));
    }

    Ok(())
}

async fn skip_current_song(
    ctx: &Context,
    msg: Message,
    playing: Arc<Mutex<bool>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialization.")
        .clone();

    if let Some(handler_lock) = manager.get(guild_id) {
        let mut handler = handler_lock.lock().await;

        handler.stop();

        let mut playing_guard = playing.lock().await;
        *playing_guard = false;
    }

    Ok(())
}
