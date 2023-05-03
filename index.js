const { Client, Intents } = require('discord.js')
const { Configuration, OpenAIApi } = require('openai')
const ytdl = require('discord-ytdl-core')
const { joinVoiceChannel, createAudioPlayer, createAudioResource, entersState, VoiceConnectionStatus } = require('@discordjs/voice')
require('dotenv').config()

const client = new Client({
    intents: [Intents.FLAGS.GUILDS, Intents.FLAGS.GUILD_MESSAGES],
})

const chatGPT_API_KEY = process.env.OPENAI_KEY
const DISCORD_BOT_TOKEN = process.env.DISCORD_KEY


const configuration = new Configuration({
    apiKey: chatGPT_API_KEY,
})
const openai = new OpenAIApi(configuration)

async function chatGPT(prompt) {

    const data = {
        model: 'text-davinci-003',
        prompt: prompt,
        max_tokens: 1000,
        temperature: 1,
    }

    try {
        const response = await openai.createCompletion(data)
        return response.data.choices[0].text.trim()
    } catch (error) {
        console.error('Error interacting with ChatGPT:', error)
        return `You did an error : ${error}`
    }
}

let connection
function isValidYouTubeURL(url) {
    const validURLRegex = /^(https?:\/\/)?(www\.)?(youtube\.com|youtu\.?be)\/.+$/
    return validURLRegex.test(url)
}

async function joinChannel(member, guild) {
    console.log(member.voice.channel.id)
    console.log(guild.id)
    return new Promise((resolve, reject) => {
        const connection = joinVoiceChannel({
            channelId: member.voice.channel.id,
            guildId: guild.id,
            adapterCreator: guild.voiceAdapterCreator,
        })

        connection.on(VoiceConnectionStatus.Ready, () => {
            resolve(connection)
        })

        connection.on(VoiceConnectionStatus.Disconnected, () => {
            reject(new Error('Disconnected from the voice channel'))
        })

        connection.on(VoiceConnectionStatus.Failed, (error) => {
            reject(error)
        })

        setTimeout(() => {
            reject(new Error('Timeout while connecting to the voice channel'))
        }, 60000)
    })
}

async function playAudio(msg, client, videoURL) {
    const guild = await client.guilds.fetch(msg.guildId)
    const member = await guild.members.fetch(msg.author.id)

    if (!member.voice.channel) {
        return msg.reply('Please join a voice channel to play audio.')
    }

    if (!isValidYouTubeURL(videoURL)) {
        return msg.reply('Please provide a valid YouTube URL.')
    }

    if (connection && connection.state.status === VoiceConnectionStatus.Playing) {
        return msg.reply('I am already playing audio in a voice channel.')
    }

    try {
        connection = await joinChannel(member, guild)

        const audioStream = ytdl(videoURL, {
            opusEncoded: true,
            encoderArgs: ['-af', 'bass=g=10,dynaudnorm=f=200'],
            fmt: 'webm',
            filter: 'audioonly',
            dlChunkSize: 0,
            highWaterMark: 1 << 25,
        })

        const audioResource = createAudioResource(audioStream, { inputType: StreamType.Opus })
        const audioPlayer = createAudioPlayer()

        audioPlayer.play(audioResource)
        const subscription = connection.subscribe(audioPlayer)

        audioPlayer.on('error', error => {
            console.error('Error playing audio:', error)
            msg.reply('An error occurred while playing the audio.')
        })

        audioPlayer.on('idle', () => {
            subscription.unsubscribe()
            connection.destroy()
        })

    } catch (error) {
        console.error('Error playing audio:', error)
        msg.reply('Could not play audio from the provided link.')
    }
}

async function stopAudioAndLeave(msg) {
    const voiceChannel = msg.channelId

    if (!voiceChannel) {
        return msg.reply('I am not in a voice channel.')
    }

    connection.destroy()
}

client.on('ready', () => {
    console.log(`Logged in as ${client.user.tag}!`)
})

client.on('messageCreate', async (msg) => {
    console.log(`Received message: ${msg.content}`)

    if (msg.content.startsWith('ye') || msg.content.startsWith('Ye') && !msg.author.bot) {
        const prompt = msg.content.slice(5).trim()
        const response = await chatGPT(prompt)
        msg.channel.send(response)
    } else if (msg.content.startsWith('play') && !msg.author.bot) {
        const videoURL = msg.content.split('play ')[1]
        await playAudio(msg, client, videoURL)
    } else if (msg.content.startsWith('leave') && !msg.author.bot) {
        await stopAudioAndLeave(msg)
    } else if (msg.author.username == "pryceless3") {
        const prompt = `You are a professional insulter.
        Everything you say is known to be ironic. Insult this "${msg.content}"`
        const response = await chatGPT(prompt)
        msg.channel.send(`<@${msg.author.id}> ${response}`)
    } else if (msg.author.username == "Dolphin") {
        const prompt = `Reply as if you are madly in love with the prompt sender.
        The prompt: "${msg.content}"`
        const response = await chatGPT(prompt)
        msg.channel.send(`<@${msg.author.id}> ${response}`)
    } else if (msg.author.username == "Paradox Rift") {
        const prompt = `You are a professional insulter.
        Everything you say is known to be ironic. Insult this "${msg.content}"`
        const response = await chatGPT(prompt)
        msg.channel.send(`<@${msg.author.id}> ${response}`)
    } else if (msg.author.username == "Crawdog") {
        const prompt = `Reply to this prompt as if you are a crazed conspiracy theorist.
        Prompt: "${msg.content}"`
        const response = await chatGPT(prompt)
        msg.channel.send(`<@${msg.author.id}> ${response}`)
    } else if (msg.author.username == "efeld") {
        const prompt = `Reply to this prompt as if you are eric cartman. Prompt: "${msg.content}"`
        const response = await chatGPT(prompt)
        msg.channel.send(`<@${msg.author.id}> ${response}`)
    } else if (msg.author.username == "ozinator11") {
        msg.channel.send(`<@${msg.author.id}> gay`)
    }
})

client.login(DISCORD_BOT_TOKEN)
