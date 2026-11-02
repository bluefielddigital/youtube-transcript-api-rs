//! Fetch through Webshare's rotating residential proxies, retrying with a
//! fresh IP when YouTube blocks a request.
//!
//! ```bash
//! cargo run --example proxy -- USERNAME PASSWORD dQw4w9WgXcQ
//! ```

use std::io::Write;

use youtube_transcript_api::YouTubeTranscriptApi;
use youtube_transcript_api::proxies::WebshareProxyConfig;

/// Fetches a transcript through Webshare with the username and password given.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(username), Some(password)) = (args.next(), args.next()) else {
        return Err("usage: proxy USERNAME PASSWORD [VIDEO_ID]".into());
    };
    let video_id = args.next().unwrap_or_else(|| "dQw4w9WgXcQ".to_owned());

    let proxy = WebshareProxyConfig::new(username, password)
        // Only use IPs in these countries.
        .with_filter_ip_locations(["us", "gb"]);
    let api = YouTubeTranscriptApi::builder()
        .proxy_config(proxy)
        .build()?;
    let transcript = api.fetch(&video_id, &["en"], false).await?;
    let mut out = std::io::stdout().lock();
    writeln!(out, "{} lines", transcript.len())?;
    Ok(())
}
