// Import required libraries
const { Client, Intents } = require('discord.js')
const { Configuration, OpenAIApi } = require('openai')
require('dotenv').config()

const client = new Client({
    intents: [Intents.FLAGS.GUILDS, Intents.FLAGS.GUILD_MESSAGES],
})

const chatGPT_API_KEY = process.env.OPENAI_KEY
const DISCORD_BOT_TOKEN = process.env.DISCORD_KEY


const configuration = new Configuration({
    apiKey: chatGPT_API_KEY,
});
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

client.on('ready', () => {
    console.log(`Logged in as ${client.user.tag}!`)
})

client.on('message', async (msg) => {
    console.log(`Received message: ${msg.content}`)

    if (msg.content.startsWith('willy') || msg.content.startsWith('Willy') && !msg.author.bot) {
        const prompt = msg.content.slice(5).trim()
        const response = await chatGPT(prompt)
        msg.channel.send(response)
    }
})

client.login(DISCORD_BOT_TOKEN)
