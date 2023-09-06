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
use std::{env, sync::Arc};
use tokio::process::Command as TokioCommand;

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
        .event_handler(Handler)
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

struct Handler;
#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        let mut message: &str = msg.content.trim();

        if msg.author.bot {
            return;
        } else if message.starts_with("ye https://") || message.starts_with("Ye https://") {
            message = message.split_at(3).1;

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
                        match extract_youtube_url(message) {
                            Ok(url) => {
                                let _ = play_youtube(&ctx, msg, url)
                                    .await
                                    .expect("Expected to play_youtube");
                            }
                            Err(_) => {
                                let _ = msg.reply(&ctx, "Bad URL").await;
                            }
                        }
                    }
                }
                None => {
                    let _ = msg.reply(&ctx, "Join a voice channel noob").await;
                }
            }
        } else if message.starts_with("Ye") || message.starts_with("ye") {
            message = message.split_at(3).1;
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
    url: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    if let Some(handler_lock) = manager.get(guild_id) {
        let mut handler = handler_lock.lock().await;

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

        handler.play_source(source);
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
