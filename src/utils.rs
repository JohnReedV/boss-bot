use serenity::{client::Context, model::prelude::ChannelId};
use songbird::input::{ChildContainer, Input, RawAdapter};
use std::{
    io,
    process::{Command, Stdio},
    time::Duration,
};
use symphonia_core::io::ReadOnlySource;
use tokio::process::Command as TokioCommand;

pub fn extract_youtube_url(input: &str) -> Result<&str, Box<dyn std::error::Error + Send + Sync>> {
    input
        .split_whitespace()
        .map(trim_url_token)
        .find(|token| is_valid_youtube_url(token))
        .ok_or_else(|| youtube_url_error().into())
}

fn youtube_url_error() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "No valid YouTube URL found")
}

fn trim_url_token(token: &str) -> &str {
    token
        .trim_end_matches(['.', ',', ';', '!', '?'])
        .trim_matches(|c| matches!(c, '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}'))
        .trim_end_matches(['.', ',', ';', '!', '?'])
}

pub fn is_valid_youtube_url(url: &str) -> bool {
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return false;
    };
    let rest = rest.strip_prefix("www.").unwrap_or(rest);

    if let Some(query) = rest.strip_prefix("youtube.com/watch?") {
        return query.split('&').any(|part| {
            part.strip_prefix("v=")
                .map(valid_youtube_id)
                .unwrap_or(false)
        });
    }

    if let Some(path) = rest.strip_prefix("youtu.be/") {
        return first_path_segment(path)
            .map(valid_youtube_id)
            .unwrap_or(false);
    }

    if let Some(path) = rest.strip_prefix("youtube.com/shorts/") {
        return first_path_segment(path)
            .map(valid_youtube_id)
            .unwrap_or(false);
    }

    false
}

fn first_path_segment(path: &str) -> Option<&str> {
    path.split(['?', '&', '/'])
        .next()
        .filter(|segment| !segment.is_empty())
}

fn valid_youtube_id(id: &str) -> bool {
    id.len() == 11
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
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
        io::Error::other(format!("yt-dlp failed to {action}"))
    } else {
        io::Error::other(format!("yt-dlp failed to {action}: {message}"))
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

pub async fn get_video_title(video_url: &str) -> Result<String, io::Error> {
    let output = TokioCommand::new("yt-dlp")
        .arg("--get-title")
        .arg(video_url)
        .output()
        .await
        .map_err(map_ytdlp_start_error)?;

    if !output.status.success() {
        return Err(ytdlp_failure("get video title", &output));
    }

    String::from_utf8(output.stdout)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

pub async fn get_video_duration(video_url: &str) -> io::Result<Duration> {
    let output = TokioCommand::new("yt-dlp")
        .arg("--get-duration")
        .arg(video_url)
        .output()
        .await
        .map_err(map_ytdlp_start_error)?;

    if !output.status.success() {
        return Err(ytdlp_failure("get video duration", &output));
    }

    let duration_str = String::from_utf8(output.stdout)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let duration_parts: Vec<&str> = duration_str.trim().split(':').collect();

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
    for part in split_large_message(message, 1950) {
        channel_id.say(&ctx.http, part).await?;
    }

    Ok(())
}

fn split_large_message(message: &str, max_length: usize) -> Vec<&str> {
    assert!(max_length > 0);

    let mut parts = Vec::new();
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
            let requested_end = std::cmp::min(start + max_length, end);
            let message_part_end = previous_char_boundary(message, start, requested_end);
            parts.push(&message[start..message_part_end]);
            start = message_part_end;
        }
    }

    parts
}

fn previous_char_boundary(message: &str, start: usize, requested_end: usize) -> usize {
    let mut end = requested_end.min(message.len());
    while end > start && !message.is_char_boundary(end) {
        end -= 1;
    }

    if end == start {
        message[start..]
            .char_indices()
            .nth(1)
            .map(|(offset, _)| start + offset)
            .unwrap_or(message.len())
    } else {
        end
    }
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

#[cfg(test)]
mod tests {
    use super::{extract_youtube_url, is_valid_youtube_url, split_large_message};

    #[test]
    fn extracts_only_the_youtube_url_token() {
        let input = "play <https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=10s>, please";
        assert_eq!(
            extract_youtube_url(input).unwrap(),
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=10s"
        );
    }

    #[test]
    fn validates_common_youtube_url_shapes() {
        assert!(is_valid_youtube_url(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        ));
        assert!(is_valid_youtube_url(
            "http://youtube.com/watch?t=1&v=dQw4w9WgXcQ"
        ));
        assert!(is_valid_youtube_url("https://youtu.be/dQw4w9WgXcQ"));
        assert!(is_valid_youtube_url(
            "https://youtube.com/shorts/dQw4w9WgXcQ?feature=share"
        ));
    }

    #[test]
    fn rejects_non_youtube_urls_and_bad_ids() {
        assert!(!is_valid_youtube_url(
            "https://example.com/watch?v=dQw4w9WgXcQ"
        ));
        assert!(!is_valid_youtube_url(
            "https://www.youtube.com/watch?v=short"
        ));
        assert!(extract_youtube_url("https://example.com nope").is_err());
    }

    #[test]
    fn splits_large_messages_on_char_boundaries() {
        let message = format!("{}💅{}", "a".repeat(1949), "b".repeat(10));
        let parts = split_large_message(&message, 1950);
        assert_eq!(parts.concat(), message);
        assert!(parts.iter().all(|part| part.len() <= 1950));
    }
}
