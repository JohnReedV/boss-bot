use openai_rust::{chat::ChatArguments, chat::Message as OpenAiMessage, Client as OpenAiClient};
use serenity::{
    async_trait,
    framework::standard::{macros::command, CommandResult, StandardFramework},
    model::{channel::Message, gateway::GatewayIntents, gateway::Ready, prelude::ChannelId},
    prelude::*,
};
use std::env;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        let mut message: &str = msg.content.trim();

        if msg.author.bot {
            return;
        } else if message.starts_with("Ye") || message.starts_with("ye") {
            message = message.split_at(3).1;
            println!("New message: {}", message);

            let api_key: String = env::var("OPENAI_KEY").expect("Expected OPENAI_KEY to be set");
            let prompt: String = message.to_string();

            let response: String = tokio::task::spawn_blocking(move || {
                let rt: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(chat_gpt(&api_key, &prompt))
            })
            .await
            .unwrap();
            println!("response: {}", response);
            let _ = send_large_message(&ctx, msg.channel_id, &response).await.expect("Expected to send_large_message");

        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        println!("Bot is ready: {}", ready.user.name);
    }
}

#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    msg.reply(ctx, "Pong!").await?;

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

#[tokio::main]
async fn main() {
    dotenv::dotenv().expect("Failed to load .env file");
    let token: String = env::var("DISCORD_KEY").expect("Expected DISCORD_KEY to be set");

    let framework: StandardFramework = StandardFramework::new().configure(|c| c.prefix("~"));

    let mut client: Client = Client::builder(&token, GatewayIntents::all())
        .framework(framework)
        .event_handler(Handler)
        .intents(GatewayIntents::all())
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}

async fn send_large_message(ctx: &Context, channel_id: ChannelId, message: &str) -> serenity::Result<()> {
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
