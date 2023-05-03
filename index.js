const { Client, Intents } = require('discord.js')
const { Configuration, OpenAIApi } = require('openai')
const ytdl = require('ytdl-core')
const { joinVoiceChannel, createAudioPlayer, createAudioResource, entersState, VoiceConnectionStatus } = require('@discordjs/voice');
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
    };

    try {
        const response = await openai.createCompletion(data)
        return response.data.choices[0].text.trim()
    } catch (error) {
        console.error('Error interacting with ChatGPT:', error)
        return `You did an error : ${error}`
    }
}

let connection;
async function playAudio(msg, videoURL) {
    if (!msg.member.voice.channel) {
        return msg.reply('Please join a voice channel to play audio.');
    }

    connection = joinVoiceChannel({
        channelId: msg.member.voice.channel.id,
        guildId: msg.guild.id,
        adapterCreator: msg.guild.voiceAdapterCreator,
    })

    try {
        await entersState(connection, VoiceConnectionStatus.Ready, 60e3);

        const audioStream = ytdl(videoURL, { filter: 'audioonly', quality: 'highestaudio', highWaterMark: 1 << 25 });
        const audioResource = createAudioResource(audioStream);
        const audioPlayer = createAudioPlayer();
        audioPlayer.play(audioResource);
        connection.subscribe(audioPlayer);

        audioPlayer.on('error', error => {
            console.error('Error playing audio:', error);
            msg.reply('An error occurred while playing the audio.');
        });
    } catch (error) {
        console.error('Error playing audio:', error);
        msg.reply('Could not play audio from the provided link.');
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
        await playAudio(msg, videoURL)
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
