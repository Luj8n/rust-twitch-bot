use dotenv;
use serde::{Deserialize, Serialize};
use std::fs;
use twitch_irc::{
  login::StaticLoginCredentials, message::ServerMessage, ClientConfig, SecureTCPTransport, TwitchIRCClient,
};

fn get_env_var(name: &str) -> String {
  return dotenv::var(name).expect(&format!("Couldn't find {} variable in the .env file", name));
}

#[derive(Serialize, Deserialize, Debug)]
struct Counter {
  name: String,
  count: i32,
}

#[derive(Serialize, Deserialize, Debug)]
struct Channel {
  name: String,
  counters: Vec<Counter>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Data {
  channels: Vec<Channel>,
}

fn add_counter(channel_name: &String, counter_name: &String, data: &mut Data) -> Result<(), String> {
  let channel = data
    .channels
    .iter_mut()
    .find(|c| &c.name == channel_name)
    .ok_or_else(|| format!("Couldn't find a channel with the name of {}", channel_name))?;

  if channel.counters.iter().any(|c| &c.name == counter_name) {
    return Err(format!("There already is a counter with the name of {}", counter_name));
  }

  channel.counters.push(Counter {
    name: counter_name.to_owned(),
    count: 0,
  });

  update_file(data);
  Ok(())
}

fn remove_counter(channel_name: &String, counter_name: &String, data: &mut Data) -> Result<(), String> {
  let channel = data
    .channels
    .iter_mut()
    .find(|c| &c.name == channel_name)
    .ok_or_else(|| format!("Couldn't find a channel with the name of {}", channel_name))?;

  channel
    .counters
    .iter_mut()
    .find(|c| &c.name == counter_name)
    .ok_or_else(|| format!("Couldn't find a counter with the name of {}", counter_name))?;

  channel.counters.retain(|c| &c.name != counter_name);

  update_file(data);
  Ok(())
}

fn add_channel(channel_name: &String, data: &mut Data) -> Result<(), String> {
  if data.channels.iter().any(|c| &c.name == channel_name) {
    return Err(format!("There already is a channel with the name of {}", channel_name));
  }

  data.channels.push(Channel {
    name: channel_name.to_owned(),
    counters: Vec::new(),
  });

  update_file(data);
  Ok(())
}

fn edit_counter(channel_name: &String, counter_name: &String, data: &mut Data, new_count: i32) -> Result<(), String> {
  let channel = data
    .channels
    .iter_mut()
    .find(|c| &c.name == channel_name)
    .ok_or_else(|| format!("Couldn't find a channel with the name of {}", channel_name))?;

  let counter = channel
    .counters
    .iter_mut()
    .find(|c| &c.name == counter_name)
    .ok_or_else(|| format!("Couldn't find a counter with the name of {}", counter_name))?;

  counter.count = new_count;

  update_file(data);
  Ok(())
}

fn pregen_data(channels: &Vec<String>, data: &mut Data) {
  for channel in channels {
    add_channel(channel, data).ok();
  }
}

fn update_file(data: &Data) {
  let stringified = serde_json::to_string_pretty(data).unwrap();
  fs::write("./data.json", stringified).unwrap();
}

#[tokio::main]
pub async fn main() {
  let mut data: Data = serde_json::from_str(&fs::read_to_string("./data.json").unwrap()).unwrap();

  let login_name = get_env_var("USERNAME");
  let oauth_token = get_env_var("OAUTH_TOKEN");
  let channels: Vec<_> = get_env_var("CHANNELS")
    .split(',')
    .map(|channel| channel.to_string().to_lowercase())
    .collect();
  let bot_prefix = get_env_var("BOT_PREFIX");

  pregen_data(&channels, &mut data);

  let config = ClientConfig::new_simple(StaticLoginCredentials::new(login_name.to_owned(), Some(oauth_token)));

  let (mut incoming_messages, client) = TwitchIRCClient::<SecureTCPTransport, StaticLoginCredentials>::new(config);

  for channel in channels {
    client.join(channel)
  }

  let join_handle = tokio::spawn(async move {
    while let Some(message) = incoming_messages.recv().await {
      match message {
        ServerMessage::Privmsg(msg) => {
          println!("#{} -> {}: {}", msg.channel_login, msg.sender.name, msg.message_text);

          let sender_is_mod = msg
            .badges
            .iter()
            .any(|badge| ["moderator", "broadcaster"].contains(&&badge.name[..]));

          let counter_name = msg.message_text.to_owned();

          match data.channels.iter().find(|c| c.name == msg.channel_login) {
            Some(channel) => match channel.counters.iter().find(|c| c.name == counter_name) {
              Some(counter) => {
                let new_count = counter.count + 1;
                match edit_counter(&msg.channel_login, &counter_name.to_owned(), &mut data, new_count) {
                  Ok(_) => {
                    client
                      .say(msg.channel_login.to_owned(), format!("{} {}", new_count, counter_name))
                      .await
                      .unwrap();
                  }
                  Err(error) => {
                    println!("Error: {}", error);
                  }
                }
              }
              None => {}
            },
            None => {}
          }

          if !msg.message_text.starts_with(&bot_prefix) {
            continue;
          }

          let words: Vec<_> = msg.message_text.split_whitespace().map(str::to_string).collect();

          match words.get(0) {
            Some(first_word) => match &first_word.to_lowercase()[1..] {
              "say" if sender_is_mod => match words.get(1) {
                Some(_) => {
                  client
                    .say(msg.channel_login.to_owned(), words[1..].join(" "))
                    .await
                    .unwrap();
                }
                None => {
                  client.reply_to_privmsg(format!("Say what?"), &msg).await.unwrap();
                }
              },
              "counter" if sender_is_mod => match words.get(1) {
                Some(second_word) => match &second_word.to_lowercase()[..] {
                  "add" => match words.get(2) {
                    Some(third_word) => match add_counter(&msg.channel_login, third_word, &mut data) {
                      Ok(_) => {
                        client
                          .reply_to_privmsg(format!("Successfully added {} counter", third_word), &msg)
                          .await
                          .unwrap();
                      }
                      Err(error) => {
                        client
                          .reply_to_privmsg(format!("Couldn't create {} counter. {}", third_word, error), &msg)
                          .await
                          .unwrap();
                      }
                    },
                    None => {
                      client
                        .reply_to_privmsg(format!("1 more argument needed"), &msg)
                        .await
                        .unwrap();
                    }
                  },
                  "remove" => match words.get(2) {
                    Some(third_word) => match remove_counter(&msg.channel_login, third_word, &mut data) {
                      Ok(_) => {
                        client
                          .reply_to_privmsg(format!("Successfully removed {} counter", third_word), &msg)
                          .await
                          .unwrap();
                      }
                      Err(error) => {
                        client
                          .reply_to_privmsg(format!("Couldn't remove {} counter. {}", third_word, error), &msg)
                          .await
                          .unwrap();
                      }
                    },
                    None => {
                      client
                        .reply_to_privmsg(format!("1 more argument needed"), &msg)
                        .await
                        .unwrap();
                    }
                  },
                  "edit" => match words.get(2) {
                    Some(third_word) => match words.get(3) {
                      Some(fourth_word) => match fourth_word.parse::<i32>() {
                        Ok(new_count) => match edit_counter(&msg.channel_login, &third_word, &mut data, new_count) {
                          Ok(_) => {
                            client
                              .reply_to_privmsg(
                                format!("Successfully set {} counter to {}", third_word, new_count),
                                &msg,
                              )
                              .await
                              .unwrap();
                          }
                          Err(error) => {
                            client
                              .reply_to_privmsg(format!("Couldn't edit {} counter. {}", third_word, error), &msg)
                              .await
                              .unwrap();
                          }
                        },
                        Err(_) => {
                          client
                            .reply_to_privmsg(format!("{} is not a number", fourth_word), &msg)
                            .await
                            .unwrap();
                        }
                      },
                      None => {
                        client
                          .reply_to_privmsg(format!("1 more arguments needed"), &msg)
                          .await
                          .unwrap();
                      }
                    },
                    None => {
                      client
                        .reply_to_privmsg(format!("2 more arguments needed"), &msg)
                        .await
                        .unwrap();
                    }
                  },
                  _ => {}
                },
                None => {}
              },
              _ => {}
            },
            None => {}
          }
        }
        _ => {}
      }
    }
  });

  join_handle.await.unwrap();
}
