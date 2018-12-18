#![feature(vec_remove_item)]

#[macro_use] extern crate serenity;
#[macro_use] extern crate mysql;

extern crate dotenv;
extern crate typemap;
extern crate reqwest;
extern crate serde;
extern crate serde_json;

use std::env;
use serenity::prelude::{Context, EventHandler, Mentionable};
use serenity::model::gateway::{Game, Ready};
use dotenv::dotenv;
use typemap::Key;
use serenity::model::id::*;
use serenity::model::channel::*;
use serenity::model::user::*;
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
            mysql.prep_exec(r#"INSERT INTO servers (id, bans) VALUES (:id, "[]")"#, params!{"id" => g.as_u64()}).unwrap();
        }
    }

    fn reaction_add(&self, ctx: Context, reaction: Reaction) {

        let data = ctx.data.lock();
        let mysql = data.get::<Globals>().unwrap();

        let mut res = mysql.prep_exec(r"SELECT id, threshold, approve_channel, upvote_emoji, downvote_emoji, role, ping FROM servers WHERE suggest_channel = :id", params!{"id" => reaction.channel_id.as_u64()}).unwrap();

        match res.next() {
            Some(r) => {
                let (g, threshold, approve_channel, upvote, downvote, role, ping) = mysql::from_row::<(u64, usize, Option<u64>, String, String, Option<u64>, Option<String>)>(r.unwrap());

                let emoji = reaction.emoji.as_data();

                if emoji == upvote || emoji == downvote {
                    let message = reaction.message().unwrap();
                    let g = GuildId::from(g);

                    if message.is_own() {
                        let content = message.content.splitn(2, "```").nth(1).unwrap().replace("```", "\n");
                        let user = reaction.user().unwrap();

                        if user.bot {
                            return ();
                        }

                        let pass = match role {
                            Some(r) => {
                                user.has_role(g, RoleId::from(r))
                            },

                            None => false,
                        };

                        if emoji == upvote {
                            let r = reaction.emoji.clone();
                            let users: Vec<User> = reaction.users::<_, UserId>(r, Some(threshold as u8 + 1), None).unwrap();

                            if users.len() > threshold || pass {

                                let channel = match approve_channel {
                                    Some(c) => {
                                        let ch = ChannelId::from(c).to_channel();
                                        let c = match ch {
                                            Ok(c) => c.id(),

                                            Err(_) => create_approve_channel(g, &mysql),
                                        };

                                        c
                                    },

                                    None => create_approve_channel(g, &mysql),
                                };

                                let _ = channel.send_message(|m| { m
                                    .embed(|e| { e
                                        .title("New Suggestion")
                                        .description(format!("{}\n\n{}", content, ping.unwrap_or(String::new())))
                                    })
                                });

                                let _ = message.delete();
                            }
                        }
                        else {

                            if pass {

                                for (id, channel) in g.to_partial_guild().unwrap().channels().unwrap() {
                                    if channel.name == "rejected-suggestions" {
                                        let _ = id.send_message(|m| { m
                                            .embed(|e| { e
                                                .title("Rejected Suggestion")
                                                .description(format!("{}", content))
                                            })
                                        });
                                    }
                                }

                                let _ = message.delete();
                            }

                        }
                    }
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
        .cmd("roleset", set_role)
        .cmd("prefix", set_prefix)
        .cmd("threshold", set_threshold)
        .cmd("ban", ban_member)
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

        let (suggest_channel, bans, upvote, downvote) = mysql::from_row::<(Option<u64>, String, String, String)>(res.unwrap());

        if bans.contains(message.author.id.as_u64().to_string().as_str()) {
            let _ = message.reply("You are banned from adding suggestions.");
        }

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

fn create_approve_channel(guild: GuildId, mysql: &mysql::Pool) -> ChannelId {
    let channel = guild.create_channel("approved-suggestions", ChannelType::Text, None);

    let id = ChannelId::from(channel.unwrap());
    let _ = mysql.prep_exec("UPDATE servers SET approve_channel = :id WHERE id = :g_id", params!{"id" => id.as_u64(), "g_id" => guild.as_u64()});

    id
}


command!(set_prefix(context, message, args) {

    match message.member().unwrap().permissions() {
        Ok(p) => {
            if !p.manage_guild() {
                let _ = message.reply("You must be a guild manager to perform this command");
            }
            else {
                let mut prefix;

                match args.single::<String>() {
                    Ok(p) => {
                        prefix = p;
                    },

                    Err(_) => {
                        let _ = message.reply("Please specify a new prefix");
                        return Ok(());
                    },
                }

                if prefix.len() > 5 {
                    let _ = message.reply("Prefix must be under 5 characters long");
                }
                else {
                    let mut data = context.data.lock();
                    let mut mysql = data.get::<Globals>().unwrap();

                    let content = format!("Prefix changed to {}", prefix);

                    mysql.prep_exec("UPDATE servers SET prefix = :prefix WHERE id = :id", params!{"prefix" => prefix, "id" => message.guild_id.unwrap().as_u64()}).unwrap();

                    let _ = message.reply(&content);
                }
            }
        },

        Err(_) => {
            return Ok(());
        },
    }
});


command!(set_threshold(context, message, args) {

    match message.member().unwrap().permissions() {
        Ok(p) => {
            if !p.manage_guild() {
                let _ = message.reply("You must be a guild manager to perform this command");
            }
            else {
                let mut threshold;

                match args.single::<u32>() {
                    Ok(p) => {
                        threshold = p;
                    },

                    Err(_) => {
                        let _ = message.reply("Please specify a natural (ℕ) threshold");
                        return Ok(());
                    },
                }

                if threshold > 100 {
                    let _ = message.reply("Please note that a threshold greater than 100 will mean suggestions can only be passed by admins.");
                    threshold = 101
                }
                let mut data = context.data.lock();
                let mut mysql = data.get::<Globals>().unwrap();

                let content = format!("Vote threshold set to {}", threshold);

                mysql.prep_exec("UPDATE servers SET threshold = :threshold WHERE id = :id", params!{"threshold" => threshold, "id" => message.guild_id.unwrap().as_u64()}).unwrap();

                let _ = message.reply(&content);
            }
        },

        Err(_) => {
            return Ok(());
        },
    }
});


command!(ban_member(context, message) {

    match message.member().unwrap().permissions() {
        Ok(p) => {
            if !p.manage_guild() {
                let _ = message.reply("You must be a guild manager to perform this command");
            }
            else {
                match message.mentions.get(0) {
                    Some(m) => {
                        let g_id = message.guild_id.unwrap();

                        let mut data = context.data.lock();
                        let mut mysql = data.get::<Globals>().unwrap();

                        let q = mysql.prep_exec("SELECT bans FROM servers WHERE id = :id", params!{"id" => g_id.as_u64()}).unwrap();
                        for res in q {
                            let bans_str = mysql::from_row::<(String)>(res.unwrap());
                            let mut bans: Vec<u64> = serde_json::from_str(&bans_str).unwrap();

                            if bans.contains(m.id.as_u64()) {
                                bans.remove_item(m.id.as_u64());
                                let _ = message.reply("User unbanned from adding suggestions.");
                            }
                            else {
                                bans.push(*m.id.as_u64());
                                let _ = message.reply("User banned from adding suggestions.");
                            }

                            mysql.prep_exec("UPDATE servers SET bans = :bans WHERE id = :id", params!{"bans" => serde_json::to_string(&bans).unwrap(), "id" => g_id.as_u64()}).unwrap();
                        }

                    },

                    None => {
                        let _ = message.reply("Please mention the user to ban.");
                    }
                }
            }
        },

        Err(_) => {
            return Ok(());
        },
    }
});


command!(set_role(context, message, args) {

    match message.member().unwrap().permissions() {
        Ok(p) => {
            if !p.manage_guild() {
                let _ = message.reply("You must be a guild manager to perform this command");
            }
            else {
                match args.single::<String>() {
                    Ok(m) => {
                        let id = m.trim_matches(|c| !char::is_numeric(c) );

                        if id.is_empty() {
                            let _ = message.reply("Please state the ID/mention of the role.");
                        }
                        else {

                            let g_id = message.guild_id.unwrap();

                            let mut data = context.data.lock();
                            let mut mysql = data.get::<Globals>().unwrap();

                            let content = format!("Auto-approve role set to <@&{}>", id);

                            mysql.prep_exec("UPDATE servers SET role = :role WHERE id = :id", params!{"role" => id, "id" => g_id.as_u64()}).unwrap();

                            let _ = message.reply(&content);
                        }
                    },

                    Err(_) => {
                        let _ = message.reply("Please state the ID/mention of the role.");
                    }
                }
            }
        },

        Err(_) => {
            return Ok(());
        },
    }
});


command!(help(_context, message) {
    let _ = message.channel_id.send_message(|m| {
        m.embed(|e| {
            e.title("Help")
            .description(r#"__**Suggestion Bot Help Menu**__

`~prefix <desired prefix>` - changes the bots prefix.
`~roleset <@role OR "off">` - Sets the role that can instantly approve suggestions.
`~ban <user>` - Ban/unban specific members from adding suggestions.
`~suggest <Custom suggestion>` - Allows users to submit suggestions. Alias: `~s`.
`~threshold <integer>` - allows you to set the number of votes a suggestion has to get before being approved.
`~upvote <emoji>` - change the upvote emoji
`~downvote <emoji>` - change the downvote emoji
`~ping <text>` - set some text to display at the base of approved suggestions

Info: want rejected suggestions to go somewhere? Make a channel called `rejected-suggestions` and we'll send them there for you."#)
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
