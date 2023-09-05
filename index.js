const { Client, Intents } = require('discord.js');
const OpenAI = require('openai')
require('dotenv').config()

const client = new Client({
    intents: [Intents.FLAGS.GUILDS, Intents.FLAGS.GUILD_MESSAGES],
})

const openai = new OpenAI({
    apiKey: process.env.OPENAI_KEY
})

async function chatGPT(prompt) {
    const params = {
        messages: [{ role: 'user', content: prompt }],
        model: 'gpt-3.5-turbo',
      };

    try {
        const response = await openai.chat.completions.create(params)
        console.log(response.choices[0].message)
        return response.choices[0].message
    } catch (error) {
        console.error('Error interacting with ChatGPT:', error)
        return `You did an error : ${error}`
    }
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
    } else if (msg.author.username == "drlangeschlange" || msg.author.username == "TheDankGuy") {
        const prompt = `Reply to this prompt as if you are having a discussion with a very important man: "${msg.content}"`
        const response = await chatGPT(prompt)
        msg.channel.send(`<@${msg.author.id}> ${response}`)
    }
})

client.login(process.env.DISCORD_KEY)
