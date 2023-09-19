use regex::Regex;
use serenity::{client::Context, model::prelude::ChannelId};
use std::{process::Command, time::Duration};
use tokio::process::Command as TokioCommand;

pub fn extract_youtube_url(input: &str) -> Result<&str, Box<dyn std::error::Error + Send>> {
    let start_index = input.find("https://www.youtube.com/watch?v=");
    match start_index {
        Some(start) => {
            let potential_url = &input[start..];
            if is_valid_youtube_url(potential_url) {
                return Ok(potential_url);
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

pub fn is_valid_youtube_url(url: &str) -> bool {
    let re = Regex::new(r"https?://(www\.)?youtube\.com/watch\?v=[a-zA-Z0-9_-]+").unwrap();
    return re.is_match(url);
}

pub async fn get_video_title(video_url: &String) -> Result<String, std::io::Error> {
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

pub async fn get_video_duration(video_url: &str) -> std::io::Result<Duration> {
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

        Ok(duration)
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "yt-dlp failed to get video duration",
        ));
    }
}

pub async fn send_large_message(
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

pub async fn get_searched_url(
    search_query: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let search_url = format!("ytsearch1:{}", search_query);
    let output = TokioCommand::new("yt-dlp")
        .arg("--default-search")
        .arg("ytsearch")
        .arg("--get-id")
        .arg(&search_url)
        .output()
        .await?;

    let output_str = String::from_utf8(output.stdout)?;
    let mut lines = output_str.lines();
    let video_id = lines.next().unwrap_or_default();
    let video_url = format!("https://www.youtube.com/watch?v={}", video_id);

    return Ok(video_url);
}
