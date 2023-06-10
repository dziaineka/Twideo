extern crate dotenvy;
extern crate twitter_video_dl;

mod helpers;

use chrono::Local;
use dotenvy::dotenv;
use helpers::{get_thread, get_twitter_data, get_twitter_id, TwitDetails, TwitterID};
use reqwest::Url;
use std::error::Error;
use std::io::Write;
use teloxide::{
    payloads::SendMessageSetters,
    prelude::*,
    types::{
        InlineKeyboardButton, InlineKeyboardMarkup, InputFile, InputMedia, InputMediaPhoto,
        InputMediaVideo, ParseMode, Recipient,
    },
};
use twitter_video_dl::serde_schemes::Variant;

struct MediaWithExtra {
    media: Vec<InputMedia>,
    extra_urls: Vec<Variant>,
    caption: String,
    allowed: bool,
    keyboard: Option<Vec<Vec<InlineKeyboardButton>>>,
}

struct TelegramTextMessage {
    text: String,
    keyboard: Option<Vec<Vec<InlineKeyboardButton>>>,
}

enum TelegramMessage {
    Media(MediaWithExtra),
    Text(TelegramTextMessage),
    Unauthorized(i32),
    TooManyRequest(i32),
    None,
}

const FULL_ALBUM: u8 = 1;
const THREAD: u8 = 2;

fn message_response_cb(twitter_data: &TwitDetails) -> TelegramMessage {
    let mut caption_is_set = false;
    let mut media_group = Vec::new();
    let mut allowed = false;

    let keyboard =
        if twitter_data.thread_count > 0 && twitter_data.next <= twitter_data.thread_count as u8 {
            Some(vec![vec![InlineKeyboardButton::callback(
                "Next tweet from thread".to_string(),
                format!(
                    "{}_{}_{}_{}",
                    THREAD, twitter_data.conversation_id, twitter_data.user_id, twitter_data.next
                ),
            )]])
        } else {
            None
        };

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
        return TelegramMessage::Text(TelegramTextMessage {
            text: twitter_data.caption.to_string(),
            keyboard,
        });
    }

    TelegramMessage::Media(MediaWithExtra {
        media: media_group,
        extra_urls: twitter_data.extra_urls.to_vec(),
        caption: twitter_data.caption.to_string(),
        allowed,
        keyboard,
    })
}

async fn convert_to_telegram<F>(url: &str, callback: F) -> TelegramMessage
where
    F: Fn(&TwitDetails) -> TelegramMessage,
{
    if let TwitterID::Id(id) = get_twitter_id(url) {
        let data = get_twitter_data(id).await;

        if data.is_ok() {
            if let Some(twitter_data) = data.unwrap() {
                return callback(&twitter_data);
            }

            return TelegramMessage::TooManyRequest(429);
        }
        return TelegramMessage::Unauthorized(401);
    }

    TelegramMessage::None
}

async fn convert_to_telegram_by_id<F>(id: u64, next: u8, callback: F) -> TelegramMessage
where
    F: Fn(&TwitDetails) -> TelegramMessage,
{
    let data = get_twitter_data(id).await;

    if data.is_ok() {
        if let Some(mut twitter_data) = data.unwrap() {
            twitter_data.next = next;
            return callback(&twitter_data);
        }

        return TelegramMessage::TooManyRequest(429);
    }

    TelegramMessage::Unauthorized(401)
}

async fn send_telegram_message<Contact>(
    message_to_send: TelegramMessage,
    message_to_reply: Option<&Message>,
    bot: &Bot,
    chat_id: Contact,
) -> Result<(), Box<dyn Error + Send + Sync>>
where
    Contact: Into<Recipient> + Copy,
{
    match message_to_send {
        TelegramMessage::Text(response) => {
            let mut telegram_message = bot
                .send_message(chat_id, response.text)
                .disable_notification(true)
                .parse_mode(ParseMode::Html)
                .disable_web_page_preview(true);

            if let Some(message_to_reply) = message_to_reply {
                telegram_message = telegram_message.reply_to_message_id(message_to_reply.id)
            }

            if let Some(keyboard) = response.keyboard {
                telegram_message
                    .reply_markup(InlineKeyboardMarkup::new(keyboard))
                    .await?;
            } else {
                telegram_message.await?;
            }
        }
        TelegramMessage::Media(media_with_extra) => {
            let mut telegram_message = bot
                .send_media_group(chat_id, media_with_extra.media)
                .disable_notification(true);

            if let Some(message_to_reply) = message_to_reply {
                telegram_message = telegram_message.reply_to_message_id(message_to_reply.id)
            }

            let response = telegram_message.await;

            if response.is_ok() {
                if let Some(keyboard) = media_with_extra.keyboard {
                    bot.send_message(chat_id, "tap button to see next thread")
                        .disable_notification(true)
                        .parse_mode(ParseMode::Html)
                        .disable_web_page_preview(true)
                        .reply_markup(InlineKeyboardMarkup::new(keyboard))
                        .await?;
                }
            } else if media_with_extra.allowed {
                // seems like too high quality video for telegram to download let's try sending
                // lower sizes
                let mut success = false;

                for variant in &media_with_extra.extra_urls {
                    let mut telegram_message = bot
                        .send_media_group(
                            chat_id,
                            [InputMedia::Video(
                                InputMediaVideo::new(InputFile::url(
                                    Url::parse(variant.url.as_str()).unwrap(),
                                ))
                                .caption(&media_with_extra.caption)
                                .parse_mode(ParseMode::Html),
                            )],
                        )
                        .disable_notification(true);

                    if let Some(message_to_reply) = message_to_reply {
                        telegram_message = telegram_message.reply_to_message_id(message_to_reply.id)
                    }

                    let response = telegram_message.await;

                    match response {
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

                    let mut telegram_message = bot
                        .send_message(chat_id, text)
                        .disable_notification(true)
                        .parse_mode(ParseMode::Html);

                    if let Some(message_to_reply) = message_to_reply {
                        telegram_message = telegram_message.reply_to_message_id(message_to_reply.id)
                    }

                    _ = telegram_message.await;
                }
            }
        }
        TelegramMessage::TooManyRequest(_code) => {
            bot.send_message(
                chat_id,
                "🧑‍💻👨‍💻⚠️ Server is busy! Please try a little later.",
            )
            .disable_web_page_preview(true)
            .await?;
        }
        TelegramMessage::Unauthorized(_code) => {
            // bot.send_message(chat_id, "☠️ Bot is stopped to work due to Twitter's new API plan(<a href='https://twitter.com/TwitterDev/status/1641222786894135296'>click to see announcement</a>). But don't despair. 👀 I'm looking for a way to come back. Be patient 💪🏻")
            // .parse_mode(ParseMode::Html)
            // .disable_web_page_preview(true)
            // .await?;
        }
        _ => (),
    }

    Ok(())
}

async fn message_handler(message: Message, bot: Bot) -> Result<(), Box<dyn Error + Send + Sync>> {
    let chat = &message.chat;

    let futures = if let Some(text) = message.text() {
        if !text.contains("twitter") {
            return Ok(());
        };

        text.split_ascii_whitespace()
            .filter(|part| part.contains("twitter"))
            .map(|part| convert_to_telegram(part, message_response_cb))
            .collect()
    } else {
        vec![]
    };

    for future in futures {
        let content_to_send = future.await;
        send_telegram_message(content_to_send, Some(&message), &bot, chat.id).await?;
    }

    Ok(())
}

async fn callback_queries_handler(
    q: CallbackQuery,
    bot: Bot,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let query = q.data.unwrap();
    let query_parts = query.split('_').collect::<Vec<&str>>();
    let query_type = query_parts[0].parse::<u8>().unwrap();

    match query_type {
        FULL_ALBUM => {
            // query template: <query-type>_<tweet-id>
            let tid = query_parts[1].parse::<u64>().unwrap();
            let response = convert_to_telegram_by_id(tid, 1, message_response_cb).await;
            send_telegram_message(response, None, &bot, q.from.id).await?;
        }
        THREAD => {
            if let Some(pressed_message) = q.message {
                _ = bot
                    .edit_message_reply_markup(pressed_message.chat.id, pressed_message.id)
                    .await;

                // query template: <query-type>_<conversation-id>_<user-id>_<thread-number>
                let conversation_id = query_parts[1].parse::<u64>().unwrap();
                let user_id = query_parts[2].parse::<u64>().unwrap();
                let thread_number = query_parts[3].parse::<u8>().unwrap();

                let tid = get_thread(conversation_id, thread_number, user_id).await;

                if let Some(tweet_id) = tid {
                    let response =
                        convert_to_telegram_by_id(tweet_id, thread_number + 1, message_response_cb)
                            .await;
                    send_telegram_message(
                        response,
                        Some(&pressed_message),
                        &bot,
                        pressed_message.chat.id,
                    )
                    .await?;
                } else {
                    bot.send_message(pressed_message.chat.id, "Thread not found 🤷‍♂️")
                        .reply_to_message_id(pressed_message.id)
                        .disable_notification(true)
                        .await?;
                }
            };
        }
        _ => {}
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
