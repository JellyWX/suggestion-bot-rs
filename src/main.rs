#[macro_use] extern crate serenity;
#[macro_use] extern crate mysql;

extern crate dotenv;
extern crate typemap;
extern crate reqwest;

use std::env;
use serenity::prelude::EventHandler;
use serenity::model::gateway::{Game, Ready};
use serenity::prelude::Context;
use dotenv::dotenv;
use typemap::Key;
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

    fn message(&self, ctx: Context, message: serenity::model::channel::Message) {
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
            mysql.prep_exec("INSERT INTO servers (id, prefix, threshold, bans) VALUES (:id, \"~\", 10, \"[]\")", params!{"id" => g.as_u64()}).unwrap();
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

    let g = match message.guild_id {
        Some(g) => g,

        None => return Ok(()),
    };

    let m = match message.member() {
        Some(m) => m,

        None => return Ok(()),
    };

    let mut data = context.data.lock();
    let mut mysql = data.get::<Globals>().unwrap();

    for res in mysql.prep_exec(r"SELECT suggest_channel, bans, upvote_emoji, downvote_emoji FROM servers WHERE id = :id", params!{"id" => g.as_u64()}).unwrap() {

        let (suggest_channel, bans, upvote_emoji, downvote_emoji) = mysql::from_row::<(Option<u64>, String, Option<String>, Option<String>)>(res.unwrap());

        if bans.contains(message.author.id.as_u64().to_string().as_str()) {
            let _ = message.reply("You are banned from adding suggestions.");
        }

        let upvote = match upvote_emoji {
            Some(e) => e,

            None => String::from("\u{002705}"),
        };

        let downvote = match downvote_emoji {
            Some(e) => e,

            None => String::from("\u{00274E}"),
        };

        if let None = suggest_channel {
            // create channel
        }
        else {
            for (channel, _) in g.channels().unwrap() {
                if Some(channel.as_u64()) == suggest_channel {
                    println!("Found");
                }
            }
        }
    }
});


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
