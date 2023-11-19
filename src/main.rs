use regex::Regex;
use serenity::{
    async_trait,
    client::{Client, EventHandler},
    framework::StandardFramework,
    model::{
        channel::{Message, ReactionType},
        gateway::GatewayIntents,
    },
    prelude::Context,
};
use songbird::{SerenityInit, Songbird};
use std::{env, sync::Arc};
pub mod resources;
use resources::*;
pub mod utils;
use utils::*;
pub mod systems;
use systems::{chat_gpt, dalle_image, loop_song, manage_queue, say_queue, skip_all_enabled};
use tokio::time::{sleep, Duration};

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
            let queue = VIDEO_QUEUE.lock().await;
            let queue_clone = queue.clone();
            drop(queue);

            say_queue(msg.clone(), &ctx, queue_clone).await;
        } else if message.starts_with("! skip") || message.starts_with("!skip") {
            skip_all_enabled(self.clone(), guild_id, manager).await;
        } else if message.starts_with("! leave") || message.starts_with("!leave") {
            if manager.get(guild_id).is_some() {
                manager.remove(guild_id).await.unwrap();
                skip_all_enabled(self.clone(), guild_id, manager).await;
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
        } else if message.starts_with("! play ") || message.starts_with("!play ") {
            let query = message.split_at(5).1;
            let url = get_searched_url(query).await.unwrap();

            manage_queue(url.as_str(), msg.clone(), guild_id, &ctx, manager, self).await;
        } else if message.starts_with("! image") || message.starts_with("!image") {
            let api_key: String = env::var("OPENAI_KEY").expect("Expected OPENAI_KEY to be set");
            let query = message.split_at(5).1;
            let prompt: String = query.to_string();

            let bot_msg = msg
                .reply(&ctx, "Select a size")
                .await
                .expect("Expected prompt message");

            let sizes = vec!["🟦", "↔", "↕️"];
            for emoji in &sizes {
                bot_msg
                    .react(&ctx.http, ReactionType::Unicode(emoji.to_string()))
                    .await
                    .expect("Expected to react");
            }

            let mut selected_size = None;
            while selected_size.is_none() {
                for emoji in &sizes {
                    let reaction_type = ReactionType::Unicode(emoji.to_string());
                    if let Ok(users) = bot_msg
                        .reaction_users(&ctx.http, reaction_type, None, None)
                        .await
                    {
                        if users.iter().any(|user| user.id == msg.author.id) {
                            selected_size = match *emoji {
                                "🟦" => Some("1024x1024"),
                                "↔" => Some("1792x1024"),
                                "↕️" => Some("1024x1792"),
                                _ => None,
                            };
                            break;
                        }
                    }
                }
                if selected_size.is_none() {
                    sleep(Duration::from_secs(1)).await;
                }
            }

            let selected_size: &str = selected_size.unwrap();
            
            bot_msg.delete(&ctx).await.expect("expected to delete");
            let bot_msg_2 = msg
                .reply(&ctx, "Generating...")
                .await
                .unwrap();

            let response = tokio::task::spawn_blocking(move || {
                let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(dalle_image(&api_key, &prompt, selected_size))
            })
            .await;

            match response {
                Ok(res) => {
                    bot_msg_2.delete(&ctx).await.unwrap();
                    msg.reply(&ctx, res).await.expect("Expected prompt message");
                }
                Err(_) => {
                    bot_msg_2.delete(&ctx).await.unwrap();
                    msg.reply(&ctx, "Strait up failed to generate that").await.unwrap();
                }
            }

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
