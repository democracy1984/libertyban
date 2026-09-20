use sqlx::SqlitePool;
use teloxide::{prelude::*, utils::command::BotCommands};

use crate::db;

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum Command {
    Help,
}

const OWNER_HELP: &str = "Команды владельца:\n/help\n/giveadmin\n/ban\n/unban\n/report";
const ADMIN_HELP: &str = "Команды администратора:\n/help\n/ban\n/unban\n/report";
const USER_HELP: &str = "Команды:\n/help\n/report";

pub async fn run(bot: Bot, pool: SqlitePool) {
    let handler = Update::filter_message()
        .filter_command::<Command>()
        .endpoint(handle);

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![pool])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

async fn handle(bot: Bot, msg: Message, cmd: Command, pool: SqlitePool) -> ResponseResult<()> {
    match cmd {
        Command::Help => {
            // у сообщения может не быть автора (например, от имени канала)
            let Some(user) = &msg.from else {
                return Ok(());
            };
            if cfg!(debug_assertions) {
                println!("/help from user {} in chat {}", user.id.0, msg.chat.id.0);
            }
            let role = match db::get_role(&pool, msg.chat.id.0, user.id.0 as i64).await {
                Ok(role) => role,
                Err(e) => {
                    eprintln!("DB error: {e}");
                    None
                }
            };

            let text = match role.as_deref() {
                Some("owner") => OWNER_HELP,
                Some("admin") => ADMIN_HELP,
                _ => USER_HELP,
            };
            bot.send_message(msg.chat.id, text).await?;
        }
    }
    Ok(())
}
