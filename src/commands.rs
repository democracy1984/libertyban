use crate::db;
use sqlx::SqlitePool;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId, ParseMode, User};
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
    Report,
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
        Command::Report => report(&bot, &msg, &pool).await,
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

    // log::debug!("/help from user {} in chat {}", user.id.0, msg.chat.id.0);

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

    let chat = msg.chat.id.0;
    let by = user.id.0;
    let target_id = target.id.0;

    let text = match db::add_admin(pool, chat, target_id as i64).await {
        Ok(true) => {
            log::info!("give_admin chat={chat} by={by} target={target_id}");
            format!("{} теперь администратор", mention(target))
        }
        Ok(false) => "Этот человек уже админ или владелец".to_string(),
        Err(e) => {
            log::error!("give_admin chat={chat} by={by} target={target_id}: DB error: {e}");
            "Внутренняя ошибка, попробуй позже".to_string()
        }
    };

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

    let chat = msg.chat.id.0;
    let by = user.id.0;
    let target_id = target.id.0;

    let text = match db::remove_admin(pool, chat, target_id as i64).await {
        Ok(true) => {
            log::info!("remove_admin chat={chat} by={by} target={target_id}");
            format!("{} больше не администратор", mention(target))
        }
        Ok(false) => "Он и не был админом".to_string(),
        Err(e) => {
            log::error!("remove_admin chat={chat} by={by} target={target_id}: DB error: {e}");
            "Внутренняя ошибка, попробуй позже".to_string()
        }
    };
    bot.send_message(msg.chat.id, text)
        .parse_mode(ParseMode::Html)
        .await?;
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
        Some(user.id.0 as i64),
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
        Some(r) => format!("{} забанен. Причина: {}", mention(target), escape(r)),
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
        .only_if_banned(true)
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
    bot.send_message(msg.chat.id, text)
        .parse_mode(ParseMode::Html)
        .await?;
    Ok(())
}

async fn report(bot: &Bot, msg: &Message, pool: &SqlitePool) -> ResponseResult<()> {
    let Some(user) = &msg.from else {
        return Ok(());
    };
    let Some(reply) = msg.reply_to_message() else {
        bot.send_message(msg.chat.id, "Ответь этой командой на сообщение нарушителя")
            .await?;
        return Ok(());
    };
    let Some(target) = &reply.from else {
        return Ok(());
    };
    if target.is_bot {
        bot.send_message(msg.chat.id, "Этого пользователя нельзя репортить")
            .await?;
        return Ok(());
    }
    if target.id == user.id {
        bot.send_message(msg.chat.id, "Зачем тебе репортить самого себя?")
            .await?;
        return Ok(());
    }
    if role_of(pool, msg.chat.id, target.id).await.is_some() {
        bot.send_message(msg.chat.id, "Админов и владельца репортить нельзя")
            .await?;
        return Ok(());
    }

    let reason = reason_of(msg);

    // есть ли уже активное голосование против этого человека
    let existing = match db::active_vote(pool, msg.chat.id.0, target.id.0 as i64).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("DB error: {e}");
            bot.send_message(msg.chat.id, "Внутренняя ошибка, попробуй позже")
                .await?;
            return Ok(());
        }
    };

    let vote_id = if let Some(vote_id) = existing {
        // голосование уже идёт: добавляем свой голос
        match db::add_vote(pool, vote_id, user.id.0 as i64, reason.as_deref()).await {
            Ok(true) => {}
            Ok(false) => {
                bot.send_message(msg.chat.id, "Ты уже голосовал за этого человека")
                    .await?;
                return Ok(());
            }
            Err(e) => {
                eprintln!("DB error: {e}");
                bot.send_message(msg.chat.id, "Внутренняя ошибка, попробуй позже")
                    .await?;
                return Ok(());
            }
        }
        vote_id
    } else {
        // голосования нет: проверяем лимит (он только на новые голосования)
        let (limit, hours) = match db::get_limits(pool, msg.chat.id.0).await {
            Ok(Some(limits)) => limits,
            Ok(None) => (5, 6),
            Err(e) => {
                eprintln!("DB error: {e}");
                bot.send_message(msg.chat.id, "Внутренняя ошибка, попробуй позже")
                    .await?;
                return Ok(());
            }
        };
        let since = db::now() - hours * 3600;
        match db::count_reports_since(pool, msg.chat.id.0, user.id.0 as i64, since).await {
            Ok(n) if n >= limit => {
                bot.send_message(
                    msg.chat.id,
                    format!(
                        "Репортить больше не можешь: лимит {limit} репортов за {hours} ч. \
                         Это мера против врагов демократии"
                    ),
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

        // создаём голосование, автор сразу первый голос
        match db::start_vote(
            pool,
            msg.chat.id.0,
            target.id.0 as i64,
            user.id.0 as i64,
            reply.id.0 as i64,
            reason.as_deref(),
        )
        .await
        {
            Ok(id) => id,
            Err(e) => {
                eprintln!("DB error: {e}");
                bot.send_message(msg.chat.id, "Внутренняя ошибка, попробуй позже")
                    .await?;
                return Ok(());
            }
        }
    };

    // считаем голоса и смотрим, не пора ли банить
    let (count, required) = match (
        db::vote_count(pool, vote_id).await,
        db::required_votes(pool, msg.chat.id.0).await,
    ) {
        (Ok(c), Ok(r)) => (c, r),
        _ => {
            bot.send_message(msg.chat.id, "Внутренняя ошибка, попробуй позже")
                .await?;
            return Ok(());
        }
    };

    if count >= required {
        if let Err(e) = bot.ban_chat_member(msg.chat.id, target.id).await {
            eprintln!("Ban failed: {e}");
            bot.send_message(
                msg.chat.id,
                "Голосование набрало кворум, но бан не удался. \
                 Бот должен быть админом с правом блокировать участников",
            )
            .await?;
            return Ok(());
        }
        if let Err(e) = db::ban_user(pool, msg.chat.id.0, target.id.0 as i64, None, None).await {
            eprintln!("DB error: {e}");
        }
        if let Ok(Some(old_id)) = db::vote_message(pool, vote_id).await {
            let _ = bot
                .delete_message(msg.chat.id, MessageId(old_id as i32))
                .await;
        }
        if let Err(e) = bot.delete_message(msg.chat.id, reply.id).await {
            eprintln!("Delete failed: {e}");
        }
        bot.send_message(
            msg.chat.id,
            format!(
                "{} забанен голосованием ({count}/{required})",
                mention(target)
            ),
        )
        .parse_mode(ParseMode::Html)
        .await?;
    } else {
        send_vote_message(bot, pool, msg.chat.id, vote_id, target, count, required).await?;
    }
    Ok(())
}

async fn send_vote_message(
    bot: &Bot,
    pool: &SqlitePool,
    chat: ChatId,
    vote_id: i64,
    target: &User,
    count: i64,
    required: i64,
) -> ResponseResult<()> {
    if let Ok(Some(old_id)) = db::vote_message(pool, vote_id).await {
        let _ = bot.delete_message(chat, MessageId(old_id as i32)).await;
    }

    let reasons = db::vote_reasons(pool, vote_id).await.unwrap_or_default();
    let mut text = format!(
        "Начато голосование за кик {}\nГолосов: {count}/{required}",
        mention(target)
    );
    if !reasons.is_empty() {
        text.push_str("\nПричины:");
        for r in &reasons {
            text.push_str(&format!("\n• {}", escape(r)));
        }
    }

    let keyboard = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
        "Поддержать",
        format!("vote:{vote_id}"),
    )]]);

    let sent = bot
        .send_message(chat, text)
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard)
        .await?;

    if let Err(e) = db::set_vote_message(pool, vote_id, sent.id.0 as i64).await {
        eprintln!("DB error: {e}");
    }
    Ok(())
}
