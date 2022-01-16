use std::{
    num::{NonZeroU64, NonZeroUsize},
    thread::sleep,
    time::Duration,
};

use clap::{AppSettings, Parser};
use serde::Deserialize;

const DELAY: Duration = Duration::from_millis(500);

type Snowflake = NonZeroU64;
type Error = Box<dyn std::error::Error>;
type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Deserialize)]
struct Message {
    id: String,
}

#[derive(Debug, Deserialize)]
struct RateLimitResponse {
    retry_after: f64,
}

fn discord(req: ureq::Request, token: &str) -> Result<ureq::Response> {
    let req = req.set("Authorization", token);
    let mut res;
    loop {
        res = req.clone().call()?;
        if res.status() != 429 {
            break;
        }
        let delay = (res.into_json::<RateLimitResponse>()?.retry_after * 1000.) as u64;
        eprintln!("rate limited, waiting {} ms", delay);
        sleep(Duration::from_millis(delay));
    }
    Ok(res)
}

#[derive(Parser, Debug)]
#[clap(
    about = clap::crate_description!(),
    version = clap::crate_version!(),
    author = clap::crate_authors!(),
    global_setting = AppSettings::InferLongArgs,
)]
struct Args {
    #[clap(help = "The id of the channel")]
    channel_id: Snowflake,

    #[clap(
        help = "The emoji to react with. Custom emojis are of the format `name:id`.",
        forbid_empty_values = true
    )]
    emoji: String,

    #[clap(
        help = "The maximum number of messages to react to.",
        short,
        long,
        default_value = "5"
    )]
    limit: NonZeroUsize,

    #[clap(help = "The id of the message to start reacting from.", short, long)]
    starting_message: Option<Snowflake>,

    #[clap(
        help = "The Discord token to use.",
        short,
        long,
        forbid_empty_values = true,
        env = "DISCORD_TOKEN",
        hide_env_values = true
    )]
    token: String,
}

fn main() -> Result<()> {
    let Args {
        channel_id,
        emoji,
        limit,
        starting_message,
        token,
    } = Args::parse();

    let messages_path = format!(
        "https://discord.com/api/v9/channels/{}/messages",
        channel_id
    );

    let mut limit = limit.get();
    let mut before = None;

    macro_rules! react {
        ($msg:expr) => {
            let _ = discord(
                ureq::put(&format!(
                    "{}/{}/reactions/{}/@me",
                    messages_path, $msg, emoji
                ))
                .set("Content-Length", "0"),
                &token,
            )?;
            sleep(DELAY);
            limit -= 1;
            before = Some($msg);
        };
    }

    if let Some(start) = starting_message {
        react!(start.to_string());
    }

    while limit > 0 {
        discord(
            {
                let req = ureq::get(&messages_path).query("limit", &limit.min(100).to_string());
                if let Some(ref before) = before {
                    req.query("before", before)
                } else {
                    req
                }
            },
            &token,
        )?
        .into_json::<Vec<Message>>()?
        .into_iter()
        .try_for_each(|m| {
            react!(m.id);
            Ok::<_, Error>(())
        })?;
    }

    Ok(())
}
