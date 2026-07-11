use crate::resources::*;
use crate::utils::*;
use serenity::{
    builder::{CreateEmbed, EditMessage},
    model::channel::Message,
    prelude::Context,
};
use std::{
    cmp::max,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::time::{sleep, Instant};

pub async fn tracker(
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

    let result = run_tracker(ctx, skip, msg, node).await;

    {
        let mut tracking = tracking_mutex.lock().await;
        *tracking = false;
    }

    if let Err(why) = result {
        println!("Tracker error: {:?}", why);
    }
}

async fn run_tracker(
    ctx: Context,
    skip: Arc<AtomicBool>,
    msg: Message,
    node: Node,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let url = node.url;
    let duration = node.duration;
    let count = max(1, duration.as_secs() / NUMBER_OF_PROGRESS_BARS);

    let mut first_update = true;
    let mut current_time = 0;
    let start_time = Instant::now();
    let mut next_tick = start_time + Duration::from_secs(count);

    let clean_video_title = match get_video_title(&url).await {
        Ok(title) => title.replace(['\n', '\r'], " "),
        Err(why) => {
            println!("Error getting video title: {:?}", why);
            url.clone()
        }
    };
    let new_content = format!("Playing: ```{}```", clean_video_title);
    let mut created_message = msg.channel_id.say(&ctx.http, new_content).await?;

    let content = format!("```{}```", clean_video_title);

    while current_time < duration.as_secs() {
        let now = Instant::now();
        if now >= next_tick {
            current_time += count;
            let duration_str = format!(
                "{}:{:02} / {}:{:02}",
                current_time / 60,
                current_time % 60,
                duration.as_secs() / 60,
                duration.as_secs() % 60
            );

            let progress = ((current_time as f64 / duration.as_secs() as f64)
                * NUMBER_OF_PROGRESS_BARS as f64)
                .floor() as usize;
            let progress = progress.min(NUMBER_OF_PROGRESS_BARS as usize);
            let progress_bar = "█".repeat(progress);
            let empty_space =
                "░".repeat((NUMBER_OF_PROGRESS_BARS as usize).saturating_sub(progress));

            let combined_field = format!("{}\n{}{}", duration_str, progress_bar, empty_space);

            let embed = CreateEmbed::new()
                .title("Now Playing")
                .description(content.clone())
                .field("Progress", combined_field, false);
            let mut edit = EditMessage::new().embed(embed);

            if first_update {
                edit = edit.content(" ");
                first_update = false;
            }

            if let Err(why) = created_message.edit(&ctx.http, edit).await {
                println!("Error editing tracker message: {:?}", why);
                break;
            }

            next_tick += Duration::from_secs(count);
        }

        if skip.load(Ordering::SeqCst) {
            skip.store(false, Ordering::SeqCst);
            break;
        }

        if current_time < duration.as_secs() {
            sleep(Duration::from_millis(100)).await;
        }
    }

    if let Err(why) = created_message.delete(&ctx.http).await {
        println!("Error deleting tracker message: {:?}", why);
    }

    Ok(())
}
