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
    #[derive(Debug)]
    pub static ref VIDEO_QUEUE: Mutex<VecDeque<Node>> = Mutex::new(VecDeque::new());
}

pub const HELP_MESSAGE: &str = "💅🏻 **Woman Commands** ☕\n\
```markdown\n\
1. !https://<URL>  -- Add a YouTube video to the queue\n\
2. !q              -- Display the current audio queue\n\
3. !skip           -- Skip the currently playing song\n\
4. !leave          -- Leave the voice channel and clear the queue\n\
5. !help           -- Displays this page\n\
6. !               -- Everything proceeding from \"!\" is a GPT prompt\n\
```";

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
    pub skip_player: Arc<AtomicBool>,
    pub skip_tracker: Arc<AtomicBool>,
}

impl Default for Handler {
    fn default() -> Self {
        Handler {
            playing: Arc::new(Mutex::new(false)),
            tracking: Arc::new(Mutex::new(false)),
            skip_player: Arc::new(AtomicBool::new(false)),
            skip_tracker: Arc::new(AtomicBool::new(false)),
        }
    }
}
