use serenity::{
    async_trait,
    client::{Client, EventHandler},
    model::{channel::Message, gateway::GatewayIntents},
    prelude::Context,
};
use songbird::SerenityInit;
use std::env;
pub mod resources;
use resources::*;
pub mod utils;
use utils::*;
pub mod systems;
use systems::{chat_gpt, generate_image, loop_song, manage_queue, say_queue, skip_all_enabled};

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    let token = env::var("DISCORD_KEY").expect("Expected a token in the environment");

    let mut client = Client::builder(&token, GatewayIntents::all())
        .event_handler(Handler::default())
        .register_songbird()
        .await
        .expect("Err creating client");

    if let Err(why) = client.start().await {
        println!("Client ended: {:?}", why);
    }
}

fn command_body(message: &str) -> Option<&str> {
    message.strip_prefix('!').map(str::trim_start)
}

fn command_arg<'a>(body: &'a str, command: &str) -> Option<&'a str> {
    if body == command {
        return Some("");
    }

    let rest = body.strip_prefix(command)?;
    let first = rest.chars().next()?;
    if !first.is_whitespace() {
        return None;
    }

    Some(rest.trim_start())
}

fn is_exact_command(body: &str, command: &str) -> bool {
    body == command
}

async fn delete_command_message(ctx: &Context, msg: &Message) {
    if let Err(why) = msg.delete(ctx).await {
        println!("Error deleting command message: {:?}", why);
    }
}

async fn openai_key_or_reply(ctx: &Context, msg: &Message) -> Option<String> {
    match env::var("OPENAI_KEY") {
        Ok(api_key) => Some(api_key),
        Err(_) => {
            let _ = msg
                .reply(ctx, "OPENAI_KEY is not configured for this bot.")
                .await;
            None
        }
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        let message: &str = msg.content.trim();
        if msg.author.bot {
            return;
        }

        let Some(body) = command_body(message) else {
            return;
        };

        println!("Got message: {}", message);

        let Some(guild_id) = msg.guild_id else {
            let _ = msg
                .reply(&ctx, "I only handle commands inside a server.")
                .await;
            return;
        };
        let manager = songbird::get(&ctx)
            .await
            .expect("Songbird Voice client placed in at initialization.")
            .clone();

        if body.starts_with("https://") || body.starts_with("http://") {
            manage_queue(body, msg.clone(), guild_id, &ctx, manager, self).await;
        } else if is_exact_command(body, "q") {
            delete_command_message(&ctx, &msg).await;
            let queue = VIDEO_QUEUE.lock().await;
            let queue_clone = queue.clone();
            drop(queue);

            say_queue(msg.clone(), &ctx, queue_clone).await;
        } else if is_exact_command(body, "skip") {
            skip_all_enabled(self, guild_id, manager).await;
        } else if is_exact_command(body, "leave") {
            if manager.get(guild_id).is_some() {
                if let Err(why) = manager.remove(guild_id).await {
                    println!("Error leaving voice channel: {:?}", why);
                }
                skip_all_enabled(self, guild_id, manager).await;
            }
            {
                let mut queue = VIDEO_QUEUE.lock().await;
                queue.clear();
            }
        } else if is_exact_command(body, "help") {
            delete_command_message(&ctx, &msg).await;
            if let Err(why) = msg.channel_id.say(&ctx.http, HELP_MESSAGE).await {
                println!("Error sending help message: {:?}", why);
            }
        } else if let Some(loop_args) = command_arg(body, "loop") {
            delete_command_message(&ctx, &msg).await;
            if loop_args.trim().is_empty() {
                let _ = msg
                    .channel_id
                    .say(&ctx.http, "Expected format: `!loop <count> <url>`")
                    .await;
                return;
            }
            if let Err(why) = loop_song(self, loop_args, msg.clone(), &ctx).await {
                println!("Error looping song: {:?}", why);
                let _ = msg
                    .channel_id
                    .say(&ctx.http, format!("Couldn't loop that song: {}", why))
                    .await;
            }
        } else if let Some(query) = command_arg(body, "play") {
            let query = query.trim();
            if query.is_empty() {
                let _ = msg.reply(&ctx, "Send a search query after `!play`.").await;
                return;
            }

            match get_searched_url(query).await {
                Ok(url) => {
                    manage_queue(url.as_str(), msg.clone(), guild_id, &ctx, manager, self).await
                }
                Err(why) => {
                    println!("Error searching YouTube: {:?}", why);
                    let _ = msg
                        .reply(&ctx, format!("Couldn't search YouTube: {}", why))
                        .await;
                }
            }
        } else if let Some(prompt) = command_arg(body, "image") {
            let Some(api_key) = openai_key_or_reply(&ctx, &msg).await else {
                return;
            };
            let prompt = prompt.trim();
            if prompt.is_empty() {
                let _ = msg
                    .reply(&ctx, "Send an image prompt after `!image`.")
                    .await;
                return;
            }

            let msg_clone = msg.clone();
            let ctx_clone = ctx.clone();

            if let Err(why) = generate_image(ctx_clone, msg_clone, &api_key, prompt).await {
                let _ = msg.reply(&ctx, why).await;
            }
        } else {
            let Some(api_key) = openai_key_or_reply(&ctx, &msg).await else {
                return;
            };

            let prompt = if let Some(attachment) = msg.attachments.first() {
                if attachment.filename.ends_with(".txt") {
                    match attachment.download().await {
                        Ok(content) => String::from_utf8_lossy(&content).into_owned(),
                        Err(why) => {
                            let _ = msg
                                .reply(&ctx, format!("Couldn't download that attachment: {}", why))
                                .await;
                            return;
                        }
                    }
                } else {
                    body.to_string()
                }
            } else {
                body.to_string()
            };

            if prompt.trim().is_empty() {
                let _ = msg.reply(&ctx, "Send a prompt after `!`.").await;
                return;
            }

            let response: String = chat_gpt(&api_key, &prompt).await;

            if let Err(why) = send_large_message(&ctx, msg.channel_id, &response).await {
                println!("Error sending GPT response: {:?}", why);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{command_arg, command_body, is_exact_command};

    #[test]
    fn command_body_trims_only_after_bang() {
        assert_eq!(command_body("! hello"), Some("hello"));
        assert_eq!(command_body("!hello"), Some("hello"));
        assert_eq!(command_body("hello"), None);
    }

    #[test]
    fn exact_commands_do_not_match_longer_words() {
        assert!(is_exact_command("q", "q"));
        assert!(!is_exact_command("question", "q"));
        assert!(!is_exact_command("skip please", "skip"));
    }

    #[test]
    fn command_args_require_word_boundary() {
        assert_eq!(command_arg("image cats", "image"), Some("cats"));
        assert_eq!(command_arg("image", "image"), Some(""));
        assert_eq!(command_arg("imageboard cats", "image"), None);
    }
}
