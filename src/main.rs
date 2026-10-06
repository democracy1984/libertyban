mod commands;
mod config;
mod db;

use teloxide::prelude::*;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(
        env_logger::Env::default()
            .default_filter_or("warn,libertyban=debug,teloxide::dispatching::dispatcher=error"),
    )
    .init();
    std::panic::set_hook(Box::new(|info| log::error!("PANIC: {info}")));

    let config = config::load();
    let pool = db::init(&config).await;
    let bot = Bot::new(config.telegram_api_key.clone());

    log::info!("Bot is ready");

    commands::run(bot, pool).await;
}
