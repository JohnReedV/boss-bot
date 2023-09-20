use systems;
use loop_songs;
use manage_queue;
use play_youtube;

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

    let url = node.url;
    let duration = node.duration;
    let count = max(1, duration.as_secs() / NUMBER_OF_PROGRESS_BARS);

    let mut first_update = true;
    let mut current_time = 0;
    let start_time = Instant::now();
    let mut next_tick = start_time + Duration::from_secs(count);

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
            current_time += count;
            let duration_str = format!(
                "{}:{:02} / {}:{:02}",
                current_time / 60,
                current_time % 60,
                duration.as_secs() / 60,
                duration.as_secs() % 60
            );

            let progress: usize = ((current_time as f64 / duration.as_secs() as f64)
                * NUMBER_OF_PROGRESS_BARS as f64)
                .floor() as usize;
            let progress_bar: String = std::iter::repeat("█").take(progress).collect();
            let empty_space: String = std::iter::repeat("░")
                .take(49 - progress as usize)
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

            next_tick += Duration::from_secs(count);
        }

        if skip.load(Ordering::SeqCst) {
            skip.store(false, Ordering::SeqCst);
            break;
        }
    }
    {
        let mut tracking = tracking_mutex.lock().await;
        *tracking = false;
    }
    created_message.delete(&ctx.http).await.unwrap();
}