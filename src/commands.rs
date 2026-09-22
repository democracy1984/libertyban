use crate::db;
use sqlx::SqlitePool;
use teloxide::types::{ParseMode, User};
use teloxide::utils::html::escape;
use teloxide::{prelude::*, utils::command::BotCommands};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum Command {
    Help,
    GiveAdmin,
    RemoveAdmin,
    Ban,
    Unban(i64),
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
        Command::Ban => ban(&bot, &msg, &pool).await,
        Command::Unban(user_id) => unban(&bot, &msg, &pool, user_id).await,
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

fn mention(user: &User) -> String {
    format!(
        r#"<a href="tg://user?id={}">{}</a>"#,
        user.id.0,
        escape(&user.full_name())
    )
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
        Ok(true) => format!("{} теперь администратор", mention(target)),
        Ok(false) => "Этот человек уже админ или владелец".to_string(),
        Err(e) => {
            eprintln!("DB error: {e}");
            "Внутренняя ошибка, попробуй позже".to_string()
        }
    };

    if cfg!(debug_assertions) {
        println!("{}", text);
    }

    bot.send_message(msg.chat.id, text)
        .parse_mode(ParseMode::Html)
        .await?;
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

fn reason_of(msg: &Message) -> Option<String> {
    let text = msg.text()?;
    let (_, rest) = text.split_once(char::is_whitespace)?;
    let rest = rest.trim();
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

async fn ban(bot: &Bot, msg: &Message, pool: &SqlitePool) -> ResponseResult<()> {
    let Some(user) = &msg.from else {
        return Ok(());
    };

    // 1. только админ или владелец
    let role = role_of(pool, msg.chat.id, user.id).await;
    let is_owner = role.as_deref() == Some("owner");
    if !is_owner && role.as_deref() != Some("admin") {
        bot.send_message(msg.chat.id, "Эта команда только для админов и владельца")
            .await?;
        return Ok(());
    }

    // 2. кого банить: автор сообщения, на которое ответили
    let Some(reply) = msg.reply_to_message() else {
        bot.send_message(msg.chat.id, "Ответь этой командой на сообщение нарушителя")
            .await?;
        return Ok(());
    };
    let Some(target) = &reply.from else {
        return Ok(());
    };
    if target.is_bot || target.id == user.id {
        bot.send_message(msg.chat.id, "Этого пользователя банить нельзя")
            .await?;
        return Ok(());
    }
    if role_of(pool, msg.chat.id, target.id).await.is_some() {
        bot.send_message(msg.chat.id, "Админов и владельца банить нельзя")
            .await?;
        return Ok(());
    }

    // 3. лимит для админов (у владельца ограничений нет)
    if !is_owner {
        let (limit, hours) = match db::get_limits(pool, msg.chat.id.0).await {
            Ok(Some(limits)) => limits,
            Ok(None) => (5, 6), // чата нет в настройках: значения по умолчанию
            Err(e) => {
                eprintln!("DB error: {e}");
                bot.send_message(msg.chat.id, "Внутренняя ошибка, попробуй позже")
                    .await?;
                return Ok(());
            }
        };
        let since = db::now() - hours * 3600;
        match db::count_bans_since(pool, msg.chat.id.0, user.id.0 as i64, since).await {
            Ok(n) if n >= limit => {
                bot.send_message(
                    msg.chat.id,
                    format!("Лимит: не больше {limit} банов за {hours} ч"),
                )
                .await?;
                return Ok(());
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("DB error: {e}");
                bot.send_message(msg.chat.id, "Внутренняя ошибка, попробуй позже")
                    .await?;
                return Ok(());
            }
        }
    }

    // 4. бан в Telegram
    if let Err(e) = bot.ban_chat_member(msg.chat.id, target.id).await {
        eprintln!("Ban failed: {e}");
        bot.send_message(
            msg.chat.id,
            "Не удалось забанить. Бот должен быть админом с правом блокировать участников",
        )
        .await?;
        return Ok(());
    }

    // 5. запись в базу
    let reason = reason_of(msg);
    if let Err(e) = db::ban_user(
        pool,
        msg.chat.id.0,
        target.id.0 as i64,
        user.id.0 as i64,
        reason.as_deref(),
    )
    .await
    {
        // в Telegram бан уже прошёл, но в базу не записался
        eprintln!("DB error: {e}");
    }

    // 6. удаляем сообщение нарушителя (если ему меньше 48 часов)
    if let Err(e) = bot.delete_message(msg.chat.id, reply.id).await {
        eprintln!("Delete failed: {e}");
    }

    let text = match &reason {
        Some(r) => format!("{} забанен. Причина: {r}", mention(target)),
        None => format!("{} забанен", mention(target)),
    };
    if cfg!(debug_assertions) {
        println!("{text}");
    }
    bot.send_message(msg.chat.id, text)
        .parse_mode(ParseMode::Html)
        .await?;
    Ok(())
}

async fn unban(bot: &Bot, msg: &Message, pool: &SqlitePool, user_id: i64) -> ResponseResult<()> {
    let Some(user) = &msg.from else {
        return Ok(());
    };

    let role = role_of(pool, msg.chat.id, user.id).await;
    if role.as_deref() != Some("owner") && role.as_deref() != Some("admin") {
        bot.send_message(msg.chat.id, "Эта команда только для админов и владельца")
            .await?;
        return Ok(());
    }

    // сначала снимаем ограничение в Telegram, чтобы человек мог зайти снова
    if let Err(e) = bot
        .unban_chat_member(msg.chat.id, UserId(user_id as u64))
        .await
    {
        eprintln!("Unban failed: {e}");
        bot.send_message(msg.chat.id, "Не удалось разбанить в Telegram")
            .await?;
        return Ok(());
    }

    let text = match db::unban_user(pool, msg.chat.id.0, user_id).await {
        Ok(true) => "Пользователь разбанен".to_string(),
        Ok(false) => "Этого пользователя и не было в бане".to_string(),
        Err(e) => {
            eprintln!("DB error: {e}");
            "Внутренняя ошибка, попробуй позже".to_string()
        }
    };
    if cfg!(debug_assertions) {
        println!("{text}");
    }
    bot.send_message(msg.chat.id, text).await?;
    Ok(())
}
