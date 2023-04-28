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

client.on('ready', () => {
    console.log(`Logged in as ${client.user.tag}!`)
})

client.on('message', async (msg) => {
    console.log(`Received message: ${msg.content}`)

    if (msg.content.startsWith('ye') || msg.content.startsWith('Ye') && !msg.author.bot) {
        const prompt = msg.content.slice(5).trim()
        const response = await chatGPT(prompt)
        msg.channel.send(response)
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
