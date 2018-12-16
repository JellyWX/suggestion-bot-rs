#[macro_use] extern crate serenity;
#[macro_use] extern crate mysql;

extern crate dotenv;
extern crate typemap;
extern crate reqwest;

use std::env;
use serenity::prelude::{Context, EventHandler, Mentionable};
use serenity::model::gateway::{Game, Ready};
use dotenv::dotenv;
use typemap::Key;
use serenity::model::id::*;
use serenity::model::channel::*;
use std::collections::HashMap;


struct Globals;

impl Key for Globals {
    type Value = mysql::Pool;
}


struct Handler;

impl EventHandler for Handler {
    fn guild_create(&self, _context: Context, _guild: serenity::model::guild::Guild, _new: bool) {
        let guild_count = {
            let cache = serenity::CACHE.read();
            cache.all_guilds().len()
        };

        let c = reqwest::Client::new();
        let mut m = HashMap::new();
        m.insert("server_count", guild_count);

        let _ = c.post("https://discordbots.org/api/bots/stats").header("Authorization", env::var("DBL_TOKEN").unwrap()).header("Content-Type", "application/json").json(&m).send().unwrap();
    }

    fn guild_delete(&self, _context: Context, _guild: serenity::model::guild::PartialGuild, _full: Option<std::sync::Arc<serenity::prelude::RwLock<serenity::model::guild::Guild>>>) {
        let guild_count = {
            let cache = serenity::CACHE.read();
            cache.all_guilds().len()
        };

        let c = reqwest::Client::new();
        let mut m = HashMap::new();
        m.insert("server_count", guild_count);

        c.post("https://discordbots.org/api/bots/stats").header("Authorization", env::var("DBL_TOKEN").unwrap()).header("Content-Type", "application/json").json(&m).send().unwrap();
    }

    fn ready(&self, context: Context, _: Ready) {
        println!("Bot online!");

        context.set_game(Game::playing("@Suggestion Bot help"));
    }

    fn message(&self, ctx: Context, message: Message) {
        let g = match message.guild_id {
            Some(g) => g,

            None => return (),
        };

        let data = ctx.data.lock();
        let mysql = data.get::<Globals>().unwrap();

        let mut res = mysql.prep_exec(r"SELECT COUNT(*) FROM servers WHERE id = :id", params!{"id" => g.as_u64()}).unwrap();

        let count = match res.next() {
            Some(r) => mysql::from_row::<u32>(r.unwrap()),

            None => 0,
        };

        if count == 0 {
            mysql.prep_exec(r#"INSERT INTO servers (id, prefix, threshold, bans) VALUES (:id, "~", 10, "[]")"#, params!{"id" => g.as_u64()}).unwrap();
        }
    }

    fn reaction_add(&self, ctx: Context, reaction: Reaction) {
        let data = ctx.data.lock();
        let mysql = data.get::<Globals>().unwrap();

        let mut res = mysql.prep_exec(r"SELECT threshold, approve_channel, upvote_emoji, downvote_emoji FROM servers WHERE suggest_channel = :id", params!{"id" => reaction.channel_id.as_u64()}).unwrap();

        match res.next() {
            Some(r) => {
                let (threshold, approve_channel, upvote_emoji, downvote_emoji) = mysql::from_row::<(usize, Option<u64>, Option<String>, Option<String>)>(r.unwrap());

                let upvote = upvote_emoji.unwrap_or(String::from("\u{002705}"));
                let downvote = downvote_emoji.unwrap_or(String::from("\u{00274E}"));

                let emoji = reaction.emoji.as_data();

                if emoji == upvote {
                    let r = reaction.emoji.clone();
                    let users: Vec<User> = reaction.users::<_, UserId>(r, Some(100), None).unwrap();

                    println!("{}", users.len());
                    if users.len() > threshold + 1 {
                        println!("passed");
                    }
                }
                else if emoji == downvote {

                }
                else {
                    let _ = reaction.delete();
                }
            },

            None => {
                return ()
            },
        }
    }
}


fn main() {
    dotenv().ok();

    let token = env::var("DISCORD_TOKEN").expect("token");
    let sql_url = env::var("SQL_URL").expect("sql url");

    let mut client = serenity::client::Client::new(&token, Handler).unwrap();
    client.with_framework(serenity::framework::standard::StandardFramework::new()
        .configure(|c| c
            .dynamic_prefix(|ctx, msg| {
                Some(
                    match msg.guild_id {
                        Some(g) => {
                            let mut data = ctx.data.lock();
                            let mut mysql = data.get::<Globals>().unwrap();

                            let mut res = mysql.prep_exec(r"SELECT prefix FROM servers WHERE id = :id", params!{"id" => g.as_u64()}).unwrap();

                            let prefix = match res.next() {
                                Some(r) => mysql::from_row::<String>(r.unwrap()),

                                None => String::from("~"),
                            };

                            prefix
                        },

                        None => String::from("~"),
                    }
                )
            })
            .on_mention(true)
        )

        .cmd("help", help)
        .cmd("invite", info)
        .cmd("info", info)
        .cmd("suggest", suggest)
        .cmd("s", suggest)
    );

    let my = mysql::Pool::new(sql_url).unwrap();

    {
        let mut data = client.data.lock();
        data.insert::<Globals>(my);
    }

    if let Err(e) = client.start() {
        println!("An error occured: {:?}", e);
    }
}


command!(suggest(context, message, args) {

    let g = message.guild_id.unwrap();

    let mut data = context.data.lock();
    let mut mysql = data.get::<Globals>().unwrap();

    for res in mysql.prep_exec(r"SELECT suggest_channel, bans, upvote_emoji, downvote_emoji FROM servers WHERE id = :id", params!{"id" => g.as_u64()}).unwrap() {

        let (suggest_channel, bans, upvote_emoji, downvote_emoji) = mysql::from_row::<(Option<u64>, String, Option<String>, Option<String>)>(res.unwrap());

        if bans.contains(message.author.id.as_u64().to_string().as_str()) {
            let _ = message.reply("You are banned from adding suggestions.");
        }

        let upvote = upvote_emoji.unwrap_or(String::from("\u{002705}"));
        let downvote = downvote_emoji.unwrap_or(String::from("\u{00274E}"));

        let messages = args.rest();
        if messages.is_empty() {
            let _ = message.reply("Please type your suggestion following the command.");
        }
        else {
            let channel = match suggest_channel {
                Some(c) => {
                    let ch = ChannelId::from(c).to_channel();
                    let c = match ch {
                        Ok(c) => c.id(),

                        Err(_) => create_channel(g, &mysql),
                    };

                    c
                },

                None => create_channel(g, &mysql),
            };

            let reply = channel.send_message(|m| {
                m.content(format!("**Vote below on: ** \n```{}```\n*as suggested by {}*", messages, message.author.mention()))
            }).unwrap();

            let _ = reply.react(upvote);
            let _ = reply.react(downvote);
        }
    }
});


fn create_channel(guild: GuildId, mysql: &mysql::Pool) -> ChannelId {
    let channel = guild.create_channel("user-suggestions", ChannelType::Text, None);

    let id = ChannelId::from(channel.unwrap());
    let _ = mysql.prep_exec("UPDATE servers SET suggest_channel = :id WHERE id = :g_id", params!{"id" => id.as_u64(), "g_id" => guild.as_u64()});

    id
}


command!(help(_context, message) {
    let _ = message.channel_id.send_message(|m| {
        m.embed(|e| {
            e.title("Help")
            .description("__**Suggestion Bot Help Menu**__

        `~prefix <desired prefix>` - changes the bots prefix.
        `~roleset <@role OR \"off\">` - Sets the role that can instantly approve suggestions.
        `~ban <user>` - Ban/unban specific members from adding suggestions.
        `~suggest <Custom suggestion>` - Allows users to submit suggestions.
        `~threshold <integer>` - allows you to set the number of votes a suggestion has to get before being approved.
        `~upvote <emoji>` - change the upvote emoji
        `~downvote <emoji>` - change the downvote emoji
        `~ping <text>` - set some text to display at the base of approved suggestions

        Info: want rejected suggestions to go somewhere? Make a channel called `rejected-suggestions` and we'll send them there for you.")
        })
    });
});


command!(info(_context, message) {
    let _ = message.channel_id.send_message(|m| {
        m.embed(|e| {
            e.title("Info")
            .description("
Invite me: https://discordapp.com/oauth2/authorize?client_id=474240839900725249&scope=bot&permissions=93264

Suggestion Bot is a part of the Fusion Network:
https://discordbots.org/servers/366542432671760396

Do `~help` for more.
            ")
        })
    });
});
