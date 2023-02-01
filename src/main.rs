use config::Config;
use matrix_sdk::{
    config::SyncSettings,
    event_handler::Ctx,
    room::Room,
    ruma::{
        events::room::message::Relation,
        events::room::message::{
            InReplyTo, LocationMessageEventContent, MessageType, OriginalSyncRoomMessageEvent,
            RoomMessageEventContent, TextMessageEventContent,
        },
    },
    Client,
};
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
                println!("Echoing: {}", body);
                let content =
                    RoomMessageEventContent::text_plain(format!("I received: {:?}", body));
                room.send(content, None).await?;
            }
            MessageType::File(f) => {
                println!("Echoing: {:?}", f);
                let content = RoomMessageEventContent::text_plain(format!("I received: {:?}", f));
                room.send(content, None).await?;
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
                    };
                }
            }

            _ => { /* No-op */ }
        }
    }
    Ok(())
}

async fn login_and_sync(botconfig: BotConfig) -> anyhow::Result<()> {
    #[allow(unused_mut)]
    let mut client_builder = Client::builder().homeserver_url(botconfig.homeserver_url.clone());

    // #[cfg(feature = "sled")]
    // {
    //     // The location to save files to
    //     let home = dirs::home_dir()
    //         .expect("no home directory found")
    //         .join(".cache")
    //         .join("exif_bot");
    //     client_builder = client_builder.sled_store(home, None)?;
    // }

    // #[cfg(feature = "indexeddb")]
    // {
    //     client_builder = client_builder.indexeddb_store("exif_bot", None).await?;
    // }

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
    client.add_event_handler_context(botconfig);
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
}

impl BotConfig {
    fn new(
        username: String,
        password: String,
        homeserver_url: String,
        ignore_own_messages: bool,
    ) -> Self {
        Self {
            username,
            password,
            homeserver_url,
            ignore_own_messages,
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
    // -------------------------------------------------------
    let botconfig = BotConfig::new(username, password, homeserver_url, ignore_own_messages);

    tracing_subscriber::fmt::init();

    login_and_sync(botconfig).await?;
    Ok(())
}
