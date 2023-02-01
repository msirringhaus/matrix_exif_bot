use config::Config;
use matrix_sdk::{
    config::SyncSettings,
    event_handler::Ctx,
    room::Room,
    ruma::{
        events::room::member::StrippedRoomMemberEvent,
        events::{
            reaction,
            room::message::{
                InReplyTo, LocationMessageEventContent, MessageType, OriginalSyncRoomMessageEvent,
                RoomMessageEventContent, TextMessageEventContent,
            },
        },
        events::{reaction::ReactionEventContent, room::message::Relation},
    },
    Client,
};
use tokio::time::{sleep, Duration};
mod exif;
use crate::exif::extract_location_from_exif;

async fn on_room_message(
    event: OriginalSyncRoomMessageEvent,
    room: Room,
    client: Client,
    botconfig: Ctx<BotConfig>,
) -> anyhow::Result<()> {
    if let Room::Joined(room) = room {
        if botconfig.ignore_own_messages && Some(event.sender.as_ref()) == client.user_id() {
            // Our own message, skipping.
            println!("Skipping message from ourselves.");
            return Ok(());
        }
        match event.content.msgtype {
            MessageType::Text(TextMessageEventContent { body, .. }) => {
                if body == "!leave" {
                    let content = RoomMessageEventContent::text_plain("Bye");
                    room.send(content, None).await?;
                    room.leave().await?;
                }
            }
            MessageType::Image(f) => {
                let data = client.media().get_file(f, false).await;
                if let Ok(Some(d)) = data {
                    if let Ok(l) = extract_location_from_exif(&d) {
                        let location = LocationMessageEventContent::new(l.clone(), l);
                        let mut content =
                            RoomMessageEventContent::new(MessageType::Location(location));
                        content.relates_to = Some(Relation::Reply {
                            in_reply_to: InReplyTo::new(event.event_id),
                        });
                        room.send(content, None).await?;
                    } else {
                        let content = ReactionEventContent::new(reaction::Relation::new(
                            event.event_id,
                            "‼️".to_string(),
                        ));
                        room.send(content, None).await?;
                    }
                }
            }
            _ => { /* No-op */ }
        }
    }
    Ok(())
}

async fn on_stripped_state_member(
    room_member: StrippedRoomMemberEvent,
    client: Client,
    room: Room,
) {
    if room_member.state_key != client.user_id().unwrap() {
        return;
    }

    if let Room::Invited(room) = room {
        tokio::spawn(async move {
            println!("Autojoining room {}", room.room_id());
            let mut delay = 2;

            while let Err(err) = room.accept_invitation().await {
                // retry autojoin due to synapse sending invites, before the
                // invited user can join for more information see
                // https://github.com/matrix-org/synapse/issues/4345
                eprintln!(
                    "Failed to join room {} ({err:?}), retrying in {delay}s",
                    room.room_id()
                );

                sleep(Duration::from_secs(delay)).await;
                delay *= 2;

                if delay > 3600 {
                    eprintln!("Can't join room {} ({err:?})", room.room_id());
                    break;
                }
            }
            println!("Successfully joined room {}", room.room_id());
        });
    }
}

async fn login_and_sync(botconfig: BotConfig) -> anyhow::Result<()> {
    #[allow(unused_mut)]
    let mut client_builder = Client::builder().homeserver_url(botconfig.homeserver_url.clone());

    let client = client_builder.build().await?;
    client
        .login_username(&botconfig.username, &botconfig.password)
        .initial_device_display_name("Command bot")
        .send()
        .await?;

    println!("logged in as {}", botconfig.username);

    // An initial sync to set up state and so our bot doesn't respond to old
    // messages. If the `StateStore` finds saved state in the location given the
    // initial sync will be skipped in favor of loading state from the store
    let response = client.sync_once(SyncSettings::default()).await?;
    // add our CommandBot to be notified of incoming messages, we do this after the
    // initial sync to avoid responding to messages before the bot was running.
    client.add_event_handler_context(botconfig.clone());
    if botconfig.autojoin {
        client.add_event_handler(on_stripped_state_member);
    }
    client.add_event_handler(on_room_message);

    // since we called `sync_once` before we entered our sync loop we must pass
    // that sync token to `sync`
    let settings = SyncSettings::default().token(response.next_batch);
    // this keeps state from the server streaming in to CommandBot via the
    // EventHandler trait
    client.sync(settings).await?;

    Ok(())
}

#[derive(Debug, Clone)]
struct BotConfig {
    username: String,
    password: String,
    homeserver_url: String,
    ignore_own_messages: bool,
    autojoin: bool,
}

impl BotConfig {
    fn new(
        username: String,
        password: String,
        homeserver_url: String,
        ignore_own_messages: bool,
        autojoin: bool,
    ) -> Self {
        Self {
            username,
            password,
            homeserver_url,
            ignore_own_messages,
            autojoin,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ------- Getting the login-credentials from file -------
    // You can get them however you like: hard-code them here, env-variable,
    // tcp-connection, read from file, etc. Here, we use the config-crate to
    // load from botconfig.toml.
    // Change this file to your needs, if you want to use this example binary.
    let settings = Config::builder()
        .add_source(config::File::with_name("botconfig"))
        // Add in settings from the environment (with a prefix of BOT)
        // Eg.. `BOT_DEBUG=1 ./target/app` would set the `debug` key
        .add_source(config::Environment::with_prefix("BOT"))
        .build()?;

    let username = settings.get_string("username")?;
    let password = settings.get_string("password")?;
    let homeserver_url = settings.get_string("homeserver_url")?;
    let ignore_own_messages = settings.get_bool("ignore_own_messages").unwrap_or(false);
    let autojoin = settings.get_bool("autojoin").unwrap_or(true);
    // -------------------------------------------------------
    let botconfig = BotConfig::new(
        username,
        password,
        homeserver_url,
        ignore_own_messages,
        autojoin,
    );

    tracing_subscriber::fmt::init();

    login_and_sync(botconfig).await?;
    Ok(())
}
