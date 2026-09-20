use sqlx::SqlitePool;
use teloxide::{prelude::*, utils::command::BotCommands};

use crate::db;

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum Command {
    Help,
    GiveAdmin,
    RemoveAdmin,
}

const OWNER_HELP: &str =
    "Команды владельца:\n/help\n/giveadmin\n/removeadmin\n/ban\n/unban\n/report";
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
        Command::Help => help(&bot, &msg, &pool).await,
        Command::GiveAdmin => give_admin(&bot, &msg, &pool).await,
        Command::RemoveAdmin => remove_admin(&bot, &msg, &pool).await,
    }
}

async fn role_of(pool: &SqlitePool, chat: ChatId, user: UserId) -> Option<String> {
    match db::get_role(pool, chat.0, user.0 as i64).await {
        Ok(role) => role,
        Err(e) => {
            eprintln!("DB error: {e}");
            None
        }
    }
}

async fn help(bot: &Bot, msg: &Message, pool: &SqlitePool) -> ResponseResult<()> {
    let Some(user) = &msg.from else {
        return Ok(());
    };
    if cfg!(debug_assertions) {
        println!("/help from user {} in chat {}", user.id.0, msg.chat.id.0);
    }

    let text = match role_of(pool, msg.chat.id, user.id).await.as_deref() {
        Some("owner") => OWNER_HELP,
        Some("admin") => ADMIN_HELP,
        _ => USER_HELP,
    };
    bot.send_message(msg.chat.id, text).await?;
    Ok(())
}

async fn give_admin(bot: &Bot, msg: &Message, pool: &SqlitePool) -> ResponseResult<()> {
    let Some(user) = &msg.from else {
        return Ok(());
    };

    if role_of(pool, msg.chat.id, user.id).await.as_deref() != Some("owner") {
        bot.send_message(msg.chat.id, "Эта команда только для владельца")
            .await?;
        return Ok(());
    }

    let Some(target) = msg.reply_to_message().and_then(|m| m.from.as_ref()) else {
        bot.send_message(msg.chat.id, "Ответь этой командой на сообщение человека")
            .await?;
        return Ok(());
    };
    if target.is_bot {
        bot.send_message(msg.chat.id, "Боту права выдать нельзя")
            .await?;
        return Ok(());
    }

    let text = match db::add_admin(pool, msg.chat.id.0, target.id.0 as i64).await {
        Ok(true) => format!("{} теперь администратор", target.full_name()),
        Ok(false) => "Этот человек уже админ или владелец".to_string(),
        Err(e) => {
            eprintln!("DB error: {e}");
            "Внутренняя ошибка, попробуй позже".to_string()
        }
    };
    if cfg!(debug_assertions) {
        println!("{}", text);
    }
    bot.send_message(msg.chat.id, text).await?;
    Ok(())
}

async fn remove_admin(bot: &Bot, msg: &Message, pool: &SqlitePool) -> ResponseResult<()> {
    let Some(user) = &msg.from else {
        return Ok(());
    };

    if role_of(pool, msg.chat.id, user.id).await.as_deref() != Some("owner") {
        bot.send_message(msg.chat.id, "Эта команда только для владельца")
            .await?;
        return Ok(());
    }

    let Some(target) = msg.reply_to_message().and_then(|m| m.from.as_ref()) else {
        bot.send_message(msg.chat.id, "Ответь этой командой на сообщение человека")
            .await?;
        return Ok(());
    };

    let text = match db::remove_admin(pool, msg.chat.id.0, target.id.0 as i64).await {
        Ok(true) => format!("{} больше не администратор", target.full_name()),
        Ok(false) => "Он и не был админом".to_string(),
        Err(e) => {
            eprintln!("DB error: {e}");
            "Внутренняя ошибка, попробуй позже".to_string()
        }
    };
    if cfg!(debug_assertions) {
        println!("{}", text);
    }
    bot.send_message(msg.chat.id, text).await?;
    Ok(())
}
