extern crate dotenv;
extern crate twitter_video_dl;

mod helpers;

use dotenv::dotenv;
use helpers::{generate_code, get_twitter_data, twitt_id, TwitDetails, TwitterID};
use reqwest::Url;
use std::error::Error;
use teloxide::{
    payloads::SendMessageSetters,
    prelude2::*,
    types::{
        InlineKeyboardButton, InlineKeyboardMarkup, InlineQueryResult, InlineQueryResultArticle,
        InlineQueryResultGif, InlineQueryResultPhoto, InlineQueryResultVideo, InputFile,
        InputMedia, InputMediaPhoto, InputMediaVideo, InputMessageContent, InputMessageContentText,
        ParseMode,
    },
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
    InlineResults(Vec<InlineQueryResult>),
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

fn inline_query_response_cb(twitter_data: &TwitDetails) -> Response {
    let mut inline_result: Vec<InlineQueryResult> = Vec::new();

    if twitter_data.twitter_media.is_empty() {
        inline_result.push(InlineQueryResult::Article(
            InlineQueryResultArticle::new(
                generate_code(),
                &twitter_data.name,
                InputMessageContent::Text(
                    InputMessageContentText::new(&twitter_data.caption)
                        .parse_mode(ParseMode::Html)
                        .disable_web_page_preview(true),
                ),
            )
            .description(&twitter_data.caption),
        ));
    }

    for media in &twitter_data.twitter_media {
        match media.r#type.as_str() {
            "photo" => {
                let mut inline_photo = InlineQueryResultPhoto::new(
                    generate_code(),
                    Url::parse(&media.url).unwrap(),
                    Url::parse(&media.thumb).unwrap(),
                )
                .title(&twitter_data.name)
                .caption(&twitter_data.caption)
                .parse_mode(ParseMode::Html);

                if twitter_data.twitter_media.len() > 1 {
                    let keyboard: Vec<Vec<InlineKeyboardButton>> =
                        vec![vec![InlineKeyboardButton::callback(
                            "See Album".to_string(),
                            twitter_data.id.to_string(),
                        )]];

                    inline_photo = inline_photo.reply_markup(InlineKeyboardMarkup::new(keyboard));
                }

                inline_result.push(InlineQueryResult::Photo(inline_photo));
            }
            "video" => {
                for variant in &twitter_data.extra_urls {
                    inline_result.push(InlineQueryResult::Video(
                        InlineQueryResultVideo::new(
                            generate_code(),
                            Url::parse(variant.url.as_str()).unwrap(),
                            variant.content_type.parse().unwrap(),
                            Url::parse(&media.thumb).unwrap(),
                            format!(
                                "{} (Bitrate {})",
                                &twitter_data.name,
                                variant.bit_rate.unwrap_or(0)
                            ),
                        )
                        .caption(&twitter_data.caption)
                        .parse_mode(ParseMode::Html),
                    ));
                }
            }
            "animated_gif" => {
                for variant in &twitter_data.extra_urls {
                    inline_result.push(InlineQueryResult::Gif(
                        InlineQueryResultGif::new(
                            generate_code(),
                            Url::parse(variant.url.as_str()).unwrap(),
                            Url::parse(&media.thumb).unwrap(),
                        )
                        .caption(&twitter_data.caption)
                        .parse_mode(ParseMode::Html)
                        .title(format!(
                            "{} (Bitrate {})",
                            twitter_data.name,
                            variant.bit_rate.unwrap_or(0)
                        )),
                    ));
                }
            }
            _ => {}
        }
    }
    Response::InlineResults(inline_result)
}

async fn convert_to_tl<F>(url: &str, callback: F) -> Response
where
    F: Fn(&TwitDetails) -> Response,
{
    if let TwitterID::Id(id) = twitt_id(url) {
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

async fn message_handler(
    message: Message,
    bot: AutoSend<Bot>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let chat = &message.chat;

    if let Some(maybe_url) = message.text() {
        if maybe_url == "/start" {
            bot.send_message(chat.id, "👉  Send me a valid twitter url")
                .await?;
        } else {
            let response = convert_to_tl(maybe_url, message_response_cb).await;

            match response {
                Response::Text(caption) => {
                    bot.send_message(chat.id, caption)
                        .parse_mode(ParseMode::Html)
                        .disable_web_page_preview(true)
                        .await?;
                }
                Response::Media(media_with_extra) => {
                    let response = bot.send_media_group(chat.id, media_with_extra.media).await;

                    if response.is_err() && media_with_extra.allowed {
                        bot.send_message(
                            chat.id,
                            concat!(
                                "Telegram is unable to download high quality video.\n",
                                "I will send you other qualities."
                            )
                            .to_string(),
                        )
                        .parse_mode(ParseMode::Html)
                        .disable_web_page_preview(true)
                        .await?;

                        for variant in &media_with_extra.extra_urls {
                            bot.send_media_group(
                                chat.id,
                                [InputMedia::Video(
                                    InputMediaVideo::new(InputFile::url(
                                        Url::parse(variant.url.as_str()).unwrap(),
                                    ))
                                    .caption(&media_with_extra.caption)
                                    .parse_mode(ParseMode::Html),
                                )],
                            )
                            .await?;
                        }
                    }
                }
                _ => (),
            }
        }
    }

    Ok(())
}

async fn inline_queries_handler(
    bot: AutoSend<Bot>,
    update: InlineQuery,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let response = convert_to_tl(&update.query, inline_query_response_cb).await;
    if let Response::InlineResults(inline_result) = response {
        let req_builder = bot.answer_inline_query(update.id, inline_result);
        req_builder.await?;
    }

    Ok(())
}

async fn callback_queries_handler(
    q: CallbackQuery,
    bot: AutoSend<Bot>,
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
    pretty_env_logger::init();

    log::info!("Starting twideo");

    let bot = Bot::from_env().auto_send();

    let handler = dptree::entry()
        .branch(Update::filter_message().endpoint(message_handler))
        .branch(Update::filter_inline_query().endpoint(inline_queries_handler))
        .branch(Update::filter_callback_query().endpoint(callback_queries_handler));

    Dispatcher::builder(bot, handler)
        .default_handler(|_| async {})
        .build()
        .setup_ctrlc_handler()
        .dispatch()
        .await;
}
