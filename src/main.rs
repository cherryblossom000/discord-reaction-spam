use std::num::{NonZeroU64, NonZeroUsize};

use clap::{AppSettings, Parser};
use reqwest::{
    self,
    header::{self, HeaderMap, HeaderValue},
    RequestBuilder, StatusCode,
};
use serde::Deserialize;
use tokio::time::{sleep, Duration};

const DELAY: Duration = Duration::from_millis(500);

type Snowflake = NonZeroU64;

#[derive(Debug, Deserialize)]
struct Message {
    id: String,
}

#[derive(Debug, Deserialize)]
struct RateLimitResponse {
    retry_after: f64,
}

async fn discord(req: impl Fn() -> RequestBuilder) -> reqwest::Result<reqwest::Response> {
    let mut res;
    loop {
        res = req().send().await?;
        if res.status() != StatusCode::TOO_MANY_REQUESTS {
            break;
        }
        let delay = (res.json::<RateLimitResponse>().await?.retry_after * 1000.) as u64;
        eprintln!("rate limited, waiting {} ms", delay);
        sleep(Duration::from_millis(delay)).await;
    }
    res.error_for_status()
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
    token: HeaderValue,
}

#[tokio::main]
async fn main() -> reqwest::Result<()> {
    let Args {
        channel_id,
        emoji,
        limit,
        starting_message,
        token,
    } = Args::parse();

    let mut headers = HeaderMap::new();
    let _ = headers.insert(header::AUTHORIZATION, token);
    let client = reqwest::Client::builder()
        .default_headers(headers)
        .build()?;

    let messages_path = format!(
        "https://discord.com/api/v9/channels/{}/messages",
        channel_id
    );

    let mut limit = limit.get();
    let mut before = None;

    macro_rules! react {
        ($msg:expr) => {
            let _ = discord(|| {
                client
                    .put(format!(
                        "{}/{}/reactions/{}/@me",
                        messages_path, $msg, emoji
                    ))
                    .header(header::CONTENT_LENGTH, 0)
            })
            .await?;
            sleep(DELAY).await;
            limit -= 1;
            before = Some($msg);
        };
    }

    if let Some(start) = starting_message {
        react!(start.to_string());
    }

    while limit > 0 {
        let mut query = vec![("limit", limit.min(100).to_string())];
        if let Some(before) = before {
            query.push(("before", before))
        }
        before = None; // make borrowck happy
        for message in discord(|| client.get(&messages_path).query(&query))
            .await?
            .json::<Vec<Message>>()
            .await?
            .into_iter()
            .map(|m| m.id)
        {
            react!(message);
        }
    }

    Ok(())
}
