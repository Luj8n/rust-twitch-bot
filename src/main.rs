use dotenv;
use regex;
use twitch_irc::login::StaticLoginCredentials;
use twitch_irc::message::ServerMessage;
use twitch_irc::ClientConfig;
use twitch_irc::SecureTCPTransport;
use twitch_irc::TwitchIRCClient;

#[tokio::main]
pub async fn main() {
  let login_name = dotenv::var("USERNAME").expect("No USERNAME variable in .env");
  let oauth_token = dotenv::var("OAUTH_TOKEN").expect("No OAUTH_TOKEN variable in .env");

  let config = ClientConfig::new_simple(StaticLoginCredentials::new(login_name.to_owned(), Some(oauth_token)));

  let (mut incoming_messages, client) = TwitchIRCClient::<SecureTCPTransport, StaticLoginCredentials>::new(config);

  let channels: Vec<_> = dotenv::var("CHANNELS")
    .expect("No CHANNELS variable in .env")
    .split(',')
    .map(|s| s.to_string())
    .collect();

  for channel in channels {
    client.join(channel)
  }

  let join_handle = tokio::spawn(async move {
    while let Some(message) = incoming_messages.recv().await {
      match message {
        ServerMessage::Privmsg(msg) => {
          println!("(#{}) {}: {}", msg.channel_login, msg.sender.name, msg.message_text);

          if !msg.message_text.starts_with("!") || (&msg.sender.name).to_lowercase() == (&login_name).to_lowercase() {
            continue;
          }

          let re = regex::Regex::new(r"\w+").expect("Bad regex");
          let words: Vec<_> = re.find_iter(&msg.message_text[1..]).collect();

          match words.get(0) {
            None => {}
            Some(first_match) => {
              let command = first_match.as_str().to_lowercase();
              println!("{}", command);

              match &command[..] {
                "ping" => {
                  client
                    .say(msg.channel_login.to_owned(), "Pong!".to_owned())
                    .await
                    .expect("oof");
                }
                _ => {}
              }
            }
          }
        }
        _ => {}
      }
    }
  });

  join_handle.await.unwrap();
}
