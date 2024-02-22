use regex::Regex;
use serenity::{
    async_trait,
    client::{Client, EventHandler},
    framework::StandardFramework,
    model::{channel::Message, gateway::GatewayIntents, prelude::AttachmentType},
    prelude::Context,
};
use songbird::{SerenityInit, Songbird};
use std::{env, sync::Arc, time::Duration};
use tokio::time::sleep;
pub mod resources;
use resources::*;
pub mod utils;
use utils::*;
pub mod systems;
use systems::{
    create_image, get_image_path, loop_song, manage_queue, ollama, say_queue, skip_all_enabled,
};

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

            manage_queue(message, msg.clone(), guild_id, &ctx, manager, self).await;
        } else if message.starts_with("! q") || message.starts_with("!q") {
            msg.delete(&ctx).await.unwrap();
            let queue = VIDEO_QUEUE.lock().await;
            let queue_clone = queue.clone();
            drop(queue);

            say_queue(msg.clone(), &ctx, queue_clone).await;
        } else if message.starts_with("! skip") || message.starts_with("!skip") {
            skip_all_enabled(self, guild_id, manager).await;
        } else if message.starts_with("! leave") || message.starts_with("!leave") {
            if manager.get(guild_id).is_some() {
                manager.remove(guild_id).await.unwrap();
                skip_all_enabled(self, guild_id, manager).await;
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
            loop_song(self, message, msg.clone(), &ctx).await.unwrap();
        } else if message.starts_with("! play ") || message.starts_with("!play ") {
            let query = message.split_at(5).1;
            let url = get_searched_url(query).await.unwrap();

            manage_queue(url.as_str(), msg.clone(), guild_id, &ctx, manager, self).await;
        } else if message.starts_with("! image") || message.starts_with("!image") {
            let query = message.split_at(5).1;
            let prompt: String = query.to_string();

            let img_name: u64 = create_image(prompt).await.unwrap();
            sleep(Duration::from_secs(3)).await;
            let path = get_image_path(img_name.to_string()).await.unwrap();
            let file = AttachmentType::Path(std::path::Path::new(&path));

            if let Err(why) = msg
                .channel_id
                .send_message(&ctx.http, |m| {
                    m.add_file(file);
                    m
                })
                .await
            {
                println!("Error sending message: {:?}", why);
            }
        } else if message.starts_with("!") {
            let prompt = if let Some(attachment) = msg.attachments.first() {
                if attachment.filename.ends_with(".txt") {
                    attachment
                        .download()
                        .await
                        .map(|content| String::from_utf8_lossy(&content).into())
                        .unwrap_or_default()
                } else {
                    msg.content.clone()
                }
            } else {
                msg.content.split_at(2).1.to_string()
            };

            let response: String = ollama(prompt).await;

            send_large_message(&ctx, msg.channel_id, &response)
                .await
                .expect("Expected to send_large_message");
        }
    }
}
