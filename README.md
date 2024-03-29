# BossBot

Welcome to BossBot, your one-stop solution for music playback, queue management, and even chatting with a built-in AI! This bot aims to enhance your Discord experience, making it more interactive and fun.

## Features

### 🤖 Chat with AI

Feeling lonely? Start a conversation with our built-in AI model. 
Can be configured to work with OpenAI API or a local Ollama instance.

### 🌅 AI Image generation.

Generate an image of ANYTHING!
Can be configured to work with OpenAI API or a local comfyUI instance.

### 🎵 Music Playback

Simply paste a YouTube URL after the `!` prefix, and the bot will add it to the queue and start playing in your voice channel.

### 📜 Queue Management

Want to know what's coming up next? Type `!q` to see the current queue of songs.

## Commands List

- **Play Music**: `!https://www.youtube.com/watch?v=example`
- **Play Music by Search**: `!play <query>`
- **Show Queue**: `!q`
- **Skip Song**: `!skip`
- **Loop Song**: `!loop <count> <url>`
- **Leave Channel**: `!leave`
- **Image Generation**: `!image <prompt>`
- **Chat with AI**: `! [Your message here]`
- **Help**: `!help`

## How to Use

1. **Add the Bot**: Invite the bot to your server using the invitation link provided.
2. **Join a Voice Channel**: Make sure you're in a voice channel so the bot knows where to play the music.
3. **Configure**: Modify the config.json.example to your preferences and rename it to "config.json".
4. **Start Using Commands**: Use any of the commands listed above to start interacting with the bot.

## Dependencies

### FFMPEG

```sudo apt install ffmpeg```

### YT-DLP

[You can find the installation guide here](https://github.com/yt-dlp/yt-dlp)
