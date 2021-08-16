use dotenv;
use twitch_irc::{
  login::StaticLoginCredentials, message::ServerMessage, ClientConfig, SecureTCPTransport,
  TwitchIRCClient,
};

fn get_env_var(name: &str) -> String {
  return dotenv::var(name).expect(&format!("Couldn't find {} variable in the .env file", name));
}

#[tokio::main]
pub async fn main() {
  let login_name = get_env_var("USERNAME");
  let oauth_token = get_env_var("OAUTH_TOKEN");
  let channels: Vec<_> = get_env_var("CHANNELS")
    .split(',')
    .map(|channel| channel.to_string().to_lowercase())
    .collect();
  let bot_prefix = get_env_var("BOT_PREFIX");

  let config = ClientConfig::new_simple(StaticLoginCredentials::new(
    login_name.to_owned(),
    Some(oauth_token),
  ));

  let (mut incoming_messages, client) =
    TwitchIRCClient::<SecureTCPTransport, StaticLoginCredentials>::new(config);

  for channel in channels {
    client.join(channel)
  }

  let join_handle = tokio::spawn(async move {
    while let Some(message) = incoming_messages.recv().await {
      match message {
        ServerMessage::Privmsg(msg) => {
          println!(
            "#{} -> {}: {}",
            msg.channel_login, msg.sender.name, msg.message_text
          );

          if !msg.message_text.starts_with(&bot_prefix) {
            continue;
          }

          let sender_is_mod = (&msg.badges)
            .into_iter()
            .any(|badge| vec!["moderator", "broadcaster"].contains(&&badge.name[..]));

          let words: Vec<_> = msg.message_text[1..]
            .split_whitespace()
            .map(str::to_string)
            .collect();

          if let Some(first_word) = words.get(0) {
            let command = first_word.to_lowercase();

            println!("* got command '{}'", command);

            match &command[..] {
              "ping" => {
                client
                  .say(msg.channel_login.to_owned(), "Pong!".to_owned())
                  .await
                  .unwrap();
              }
              "say" if sender_is_mod => {
                if let None = words.get(1) {
                  continue;
                }

                client
                  .say(msg.channel_login.to_owned(), words[1..].join(" "))
                  .await
                  .unwrap();
              }
              _ => {}
            }
          }
        }
        _ => {}
      }
    }
  });

  join_handle.await.unwrap();
}
