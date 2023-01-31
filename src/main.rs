use config::Config;
use matrix_sdk::{
    config::SyncSettings,
    event_handler::Ctx,
    room::Room,
    ruma::events::room::message::{
        LocationMessageEventContent, MessageType, OriginalSyncRoomMessageEvent,
        RoomMessageEventContent, TextMessageEventContent,
    },
    Client,
};
use tokio::runtime::Runtime;
mod exif;
use crate::exif::extract_location_from_exif;

async fn on_room_message(
    event: OriginalSyncRoomMessageEvent,
    room: Room,
    client: Client,
    botconfig: Ctx<Option<Config>>,
) {
    println!("Sync: {:?}", event);
    if let Room::Joined(room) = room {
        let should_skip_message = botconfig
            .as_ref()
            .and_then(|x| x.get_bool("ignore_own_messages").ok())
            .unwrap_or(false);
        if should_skip_message && Some(event.sender.as_ref()) == client.user_id() {
            // Our own message, skipping.
            println!("Skipping message from ourselves.");
            return;
        }
        match event.content.msgtype {
            MessageType::Text(TextMessageEventContent { body, .. }) => {
                println!("Echoing: {}", body);
                let content =
                    RoomMessageEventContent::text_plain(format!("I received: {:?}", body));
                room.send(content, None).await.unwrap();
            }
            MessageType::File(f) => {
                println!("Echoing: {:?}", f);
                let content = RoomMessageEventContent::text_plain(format!("I received: {:?}", f));
                room.send(content, None).await.unwrap();
            }
            MessageType::Image(f) => {
                let data = client.media().get_file(f, false).await;
                println!("Data: {:?}", data);
                if let Ok(Some(d)) = data {
                    if let Ok(l) = extract_location_from_exif(&d) {
                        let content = RoomMessageEventContent::new(MessageType::Location(
                            LocationMessageEventContent::new(l.clone(), l),
                        ));
                        room.send(content, None).await.unwrap();
                    };
                }
            }

            _ => { /* No-op */ }
        }
    }
}

fn login_and_sync(
    homeserver_url: String,
    username: String,
    password: String,
    settings: Option<Config>,
) -> anyhow::Result<()> {
    // Create the runtime
    let rt = Runtime::new().unwrap();

    #[allow(unused_mut)]
    let mut client_builder = Client::builder().homeserver_url(homeserver_url);

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

    let client = rt.block_on(client_builder.build())?;
    rt.block_on(
        client
            .login_username(&username, &password)
            .initial_device_display_name("command bot")
            .send(),
    )?;

    println!("logged in as {username}");

    // An initial sync to set up state and so our bot doesn't respond to old
    // messages. If the `StateStore` finds saved state in the location given the
    // initial sync will be skipped in favor of loading state from the store
    let response = rt.block_on(client.sync_once(SyncSettings::default()))?;
    // add our CommandBot to be notified of incoming messages, we do this after the
    // initial sync to avoid responding to messages before the bot was running.
    client.add_event_handler_context(settings);
    client.add_event_handler(on_room_message);

    // since we called `sync_once` before we entered our sync loop we must pass
    // that sync token to `sync`
    let settings = SyncSettings::default().token(response.next_batch);
    // this keeps state from the server streaming in to CommandBot via the
    // EventHandler trait
    rt.block_on(client.sync(settings))?;
    println!("------------>>> Done syncing");

    Ok(())
}

fn main() -> anyhow::Result<()> {
    // ------- Getting the login-credentials from file -------
    // You can get them however you like: hard-code them here, env-variable,
    // tcp-connection, read from file, etc. Here, we use the config-crate to
    // load from botconfig.toml.
    // Change this file to your needs, if you want to use this example binary.
    let mut settings = config::Config::default();
    settings
        .merge(config::File::with_name("botconfig"))
        .unwrap();

    let username = settings.get_string("username").unwrap();
    let password = settings.get_string("password").unwrap();
    let homeserver_url = settings.get_string("homeserver_url").unwrap();
    // -------------------------------------------------------

    // tracing_subscriber::fmt::init();

    login_and_sync(homeserver_url, username, password, Some(settings))?;
    Ok(())
}
