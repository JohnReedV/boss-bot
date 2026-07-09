use regex::Regex;
use serenity::{client::Context, model::prelude::ChannelId};
use songbird::input::{ChildContainer, Input, RawAdapter};
use std::{
    io,
    process::{Command, Stdio},
    time::Duration,
};
use symphonia_core::io::ReadOnlySource;
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

const YTDLP_NOT_FOUND: &str =
    "yt-dlp executable not found; install yt-dlp and ensure it is in PATH";
const FFMPEG_NOT_FOUND: &str =
    "ffmpeg executable not found; install ffmpeg and ensure it is in PATH";

fn map_ytdlp_start_error(error: io::Error) -> io::Error {
    if error.kind() == io::ErrorKind::NotFound {
        io::Error::new(io::ErrorKind::NotFound, YTDLP_NOT_FOUND)
    } else {
        error
    }
}

fn map_ffmpeg_start_error(error: io::Error) -> io::Error {
    if error.kind() == io::ErrorKind::NotFound {
        io::Error::new(io::ErrorKind::NotFound, FFMPEG_NOT_FOUND)
    } else {
        error
    }
}

fn ytdlp_failure(action: &str, output: &std::process::Output) -> io::Error {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = stderr.trim();

    if message.is_empty() {
        io::Error::new(io::ErrorKind::Other, format!("yt-dlp failed to {action}"))
    } else {
        io::Error::new(
            io::ErrorKind::Other,
            format!("yt-dlp failed to {action}: {message}"),
        )
    }
}

fn parse_duration_part(part: &str) -> io::Result<u64> {
    part.parse::<u64>().map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("couldn't parse yt-dlp duration segment '{part}': {error}"),
        )
    })
}

pub async fn get_video_title(video_url: &String) -> Result<String, io::Error> {
    let output = Command::new("yt-dlp")
        .arg("--get-title")
        .arg(video_url)
        .output()
        .map_err(map_ytdlp_start_error)?;

    if !output.status.success() {
        return Err(ytdlp_failure("get video title", &output));
    }

    String::from_utf8(output.stdout)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

pub async fn get_video_duration(video_url: &str) -> io::Result<Duration> {
    let output = Command::new("yt-dlp")
        .arg("--get-duration")
        .arg(video_url)
        .output()
        .map_err(map_ytdlp_start_error)?;

    if !output.status.success() {
        return Err(ytdlp_failure("get video duration", &output));
    }

    let duration_str = String::from_utf8(output.stdout)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let duration_parts: Vec<&str> = duration_str.trim().split(":").collect();

    match duration_parts.as_slice() {
        [hrs, mins, secs] => Ok(Duration::from_secs(
            parse_duration_part(hrs)? * 3600
                + parse_duration_part(mins)? * 60
                + parse_duration_part(secs)?,
        )),
        [mins, secs] => Ok(Duration::from_secs(
            parse_duration_part(mins)? * 60 + parse_duration_part(secs)?,
        )),
        [secs] => Ok(Duration::from_secs(parse_duration_part(secs)?)),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "couldn't parse yt-dlp duration",
        )),
    }
}

pub fn create_youtube_audio_input(video_url: &str) -> io::Result<Input> {
    let mut ytdlp = Command::new("yt-dlp")
        .args([
            "-f",
            "ba[abr>0][vcodec=none]/best",
            "--no-playlist",
            "-o",
            "-",
            video_url,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(map_ytdlp_start_error)?;

    let ytdlp_stdout = ytdlp
        .stdout
        .take()
        .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "yt-dlp stdout unavailable"))?;

    let ffmpeg = match Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "warning",
            "-i",
            "pipe:0",
            "-f",
            "f32le",
            "-ac",
            "2",
            "-ar",
            "48000",
            "pipe:1",
        ])
        .stdin(Stdio::from(ytdlp_stdout))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let _ = ytdlp.kill();
            let _ = ytdlp.wait();
            return Err(map_ffmpeg_start_error(error));
        }
    };

    let process_chain = ChildContainer::new(vec![ytdlp, ffmpeg]);
    let process_source = ReadOnlySource::new(process_chain);
    Ok(RawAdapter::new(process_source, 48_000, 2).into())
}

pub async fn send_large_message(
    ctx: &Context,
    channel_id: ChannelId,
    message: &str,
) -> serenity::Result<()> {
    let max_length = 1950;
    let mut start = 0;

    while start < message.len() {
        let end;
        let next_code_block_start = message[start..].find("```");

        if let Some(index) = next_code_block_start {
            if start + index == start {
                if let Some(end_code_block) = message[start + 3..].find("```") {
                    end = std::cmp::min(start + end_code_block + 6, message.len());
                } else {
                    end = message.len();
                }
            } else {
                end = std::cmp::min(start + index, message.len());
            }
        } else {
            end = message.len();
        }

        while start < end {
            let message_part_end = std::cmp::min(start + max_length, end);
            let part = &message[start..message_part_end];
            channel_id.say(&ctx.http, part).await?;

            start = message_part_end;
        }
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
        .await
        .map_err(map_ytdlp_start_error)?;

    if !output.status.success() {
        return Err(ytdlp_failure("search YouTube", &output).into());
    }

    let output_str = String::from_utf8(output.stdout)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let video_id = output_str
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "yt-dlp returned no video id"))?;

    Ok(format!("https://www.youtube.com/watch?v={}", video_id))
}
