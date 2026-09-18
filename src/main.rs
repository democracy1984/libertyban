use std::env;
use teloxide::{prelude::*, types::ChatId};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().expect("Failed to load .env");
    let telegram_api_key = env::var("TELEGRAM_API_KEY").expect("TELEGRAM_API_KEY is not set");
    let owner_chat = env::var("OWNER_CHAT")
        .expect("OWNER_CHAT is not set")
        .parse::<i64>()
        .expect("OWNER_CHAT must be a valid number");
    let bot = Bot::new(telegram_api_key);

    bot.send_message(ChatId(owner_chat), "hi")
        .await
        .expect("Failed to send message");
}
