use crate::Regex;
use lazy_static::lazy_static;
use serenity::prelude::TypeMapKey;
use songbird::Songbird;
use std::{
    collections::VecDeque,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use tokio::sync::Mutex;

lazy_static! {
    #[derive(Debug, Clone, Copy)]
    pub static ref VIDEO_QUEUE: Mutex<VecDeque<Node>> = Mutex::new(VecDeque::new());
    pub static ref RE: Regex = Regex::new(r"-?\d+").unwrap();
}

pub const NUMBER_OF_PROGRESS_BARS: u64 = 49;

pub const HELP_MESSAGE: &str = "💅🏻 **Woman Commands** ☕\n\
```markdown\n\
1. !<URL>               -- Add a YouTube video to the queue\n\
2. !play <query>        -- Plays the first YT search result\n\
3. !loop <count> <url>  -- Loop a song \n\
4. !q                   -- Display the current audio queue\n\
5. !skip                -- Skip the currently playing song\n\
6. !leave               -- Leave the voice channel and clear the queue\n\
7. !                    -- Everything proceeding from \"!\" is a GPT prompt\n\
8. !help                -- Displays this page\n\
```";

pub const FFMPEG_OPTIONS: [&str; 6] = [
    "-reconnect",
    "1",
    "-reconnect_streamed",
    "1",
    "-reconnect_delay_max",
    "5",
];

pub const AUDIO_OPTIONS: [&str; 9] = [
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

#[derive(Debug, Clone)]
pub struct Node {
    pub url: String,
    pub duration: Duration,
}

impl Node {
    pub fn new() -> Self {
        Node {
            url: String::new(),
            duration: Duration::new(0, 0),
        }
    }
    pub fn from(url: String, duration: Duration) -> Self {
        Node {
            url: url,
            duration: duration,
        }
    }
}
pub struct SongbirdKey;

impl TypeMapKey for SongbirdKey {
    type Value = Arc<Songbird>;
}

pub struct Handler {
    pub playing: Arc<Mutex<bool>>,
    pub tracking: Arc<Mutex<bool>>,
    pub looping: Arc<Mutex<bool>>,
    pub skip_player: Arc<AtomicBool>,
    pub skip_tracker: Arc<AtomicBool>,
    pub skip_loop: Arc<AtomicBool>,
}

impl Default for Handler {
    fn default() -> Self {
        Handler {
            playing: Arc::new(Mutex::new(false)),
            tracking: Arc::new(Mutex::new(false)),
            looping: Arc::new(Mutex::new(false)),
            skip_player: Arc::new(AtomicBool::new(false)),
            skip_tracker: Arc::new(AtomicBool::new(false)),
            skip_loop: Arc::new(AtomicBool::new(false)),
        }
    }
}
