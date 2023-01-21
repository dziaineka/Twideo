extern crate dotenvy;
extern crate twitter_video_dl;

mod helpers;

use chrono::Local;
use dotenvy::dotenv;
use helpers::{get_twitter_data, get_twitter_id, TwitDetails, TwitterID};
use reqwest::Url;
use std::error::Error;
use std::io::Write;
use teloxide::{
    payloads::SendMessageSetters,
    prelude::*,
    types::{InputFile, InputMedia, InputMediaPhoto, InputMediaVideo, ParseMode},
};
use twitter_video_dl::serde_schemes::Variant;

struct MediaWithExtra {
    media: Vec<InputMedia>,
    extra_urls: Vec<Variant>,
    caption: String,
    allowed: bool,
}

enum Response {
    Media(MediaWithExtra),
    Text(String),
    None,
}

fn message_response_cb(twitter_data: &TwitDetails) -> Response {
    let mut caption_is_set = false;
    let mut media_group = Vec::new();
    let mut allowed = false;

    for media in &twitter_data.twitter_media {
        let input_file = InputFile::url(Url::parse(&media.url).unwrap());

        if media.r#type == "photo" {
            let mut tl_media = InputMediaPhoto::new(input_file);
            if !caption_is_set {
                tl_media = tl_media
                    .caption(&twitter_data.caption)
                    .parse_mode(ParseMode::Html);
                caption_is_set = true;
            }
            media_group.push(InputMedia::Photo(tl_media));
        } else if media.r#type == "video" || media.r#type == "animated_gif" {
            allowed = true;
            let mut tl_media = InputMediaVideo::new(input_file);
            if !caption_is_set {
                tl_media = tl_media
                    .caption(&twitter_data.caption)
                    .parse_mode(ParseMode::Html);
                caption_is_set = true;
            }
            media_group.push(InputMedia::Video(tl_media));
        }
    }
    if !caption_is_set {
        return Response::Text(twitter_data.caption.to_string());
    }

    Response::Media(MediaWithExtra {
        media: media_group,
        extra_urls: twitter_data.extra_urls.to_vec(),
        caption: twitter_data.caption.to_string(),
        allowed,
    })
}

async fn convert_to_telegram<F>(url: &str, callback: F) -> Response
where
    F: Fn(&TwitDetails) -> Response,
{
    if let TwitterID::Id(id) = get_twitter_id(url) {
        let data = get_twitter_data(id).await.unwrap_or(None);
        if let Some(twitter_data) = data {
            return callback(&twitter_data);
        }
    }

    Response::None
}

async fn convert_to_tl_by_id<F>(id: u64, callback: F) -> Response
where
    F: Fn(&TwitDetails) -> Response,
{
    let data = get_twitter_data(id).await.unwrap_or(None);
    if let Some(twitter_data) = data {
        return callback(&twitter_data);
    }
    Response::None
}

async fn message_handler(message: Message, bot: Bot) -> Result<(), Box<dyn Error + Send + Sync>> {
    let chat = &message.chat;

    let futures = if let Some(text) = message.text() {
        if !text.contains("twitter") {
            return Ok(());
        };

        text.split_ascii_whitespace()
            .into_iter()
            .filter(|part| part.contains("twitter"))
            .map(|part| convert_to_telegram(part, message_response_cb))
            .collect()
    } else {
        vec![]
    };

    for future in futures {
        let content_to_send = future.await;

        match content_to_send {
            Response::Text(caption) => {
                bot.send_message(chat.id, caption)
                    .reply_to_message_id(message.id)
                    .disable_notification(true)
                    .parse_mode(ParseMode::Html)
                    .disable_web_page_preview(true)
                    .await?;
            }
            Response::Media(media_with_extra) => {
                let response = bot
                    .send_media_group(chat.id, media_with_extra.media)
                    .reply_to_message_id(message.id)
                    .disable_notification(true)
                    .await;

                if response.is_err() && media_with_extra.allowed {
                    // seems like too high quality video for telegram to download let's try sending
                    // lower sizes
                    let mut success = false;

                    for variant in &media_with_extra.extra_urls {
                        let result = bot
                            .send_media_group(
                                chat.id,
                                [InputMedia::Video(
                                    InputMediaVideo::new(InputFile::url(
                                        Url::parse(variant.url.as_str()).unwrap(),
                                    ))
                                    .caption(&media_with_extra.caption)
                                    .parse_mode(ParseMode::Html),
                                )],
                            )
                            .reply_to_message_id(message.id)
                            .disable_notification(true)
                            .await;

                        match result {
                            Err(_) => success = false,

                            Ok(_) => success = true,
                        }
                    }

                    // if still failure let's send media as a link and hope
                    // telegram will preview it

                    if !success {
                        let mut text: String =
                            "🤖 failed to embed media so use link this time: ".to_owned();
                        text.push_str(&media_with_extra.extra_urls.first().unwrap().url);
                        text.push_str("\n\n");
                        text.push_str(&media_with_extra.caption);

                        bot.send_message(chat.id, text)
                            .reply_to_message_id(message.id)
                            .disable_notification(true)
                            .parse_mode(ParseMode::Html)
                            .await?;
                    }
                }
            }
            _ => (),
        }
    }

    Ok(())
}

async fn callback_queries_handler(
    q: CallbackQuery,
    bot: Bot,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let tid: u64 = q.data.unwrap().parse().unwrap();
    let response = convert_to_tl_by_id(tid, message_response_cb).await;

    if let Response::Media(media_with_extra) = response {
        bot.send_media_group(q.from.id, media_with_extra.media)
            .await?;
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    dotenv().ok();

    pretty_env_logger::formatted_builder()
        .write_style(pretty_env_logger::env_logger::WriteStyle::Auto)
        .format(|buf, record| {
            writeln!(
                buf,
                "{} [{}] - {}",
                Local::now().format("%Y-%m-%dT%H:%M:%S"),
                record.level(),
                record.args()
            )
        })
        .filter(None, log::LevelFilter::Info)
        .init();

    log::info!("Starting twideo");

    let bot = Bot::from_env();

    let handler = dptree::entry()
        .branch(Update::filter_message().endpoint(message_handler))
        .branch(Update::filter_callback_query().endpoint(callback_queries_handler));

    Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}
