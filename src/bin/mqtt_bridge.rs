//! MQTT -> BLE bridge for Lotus Lantern LED controllers.
//! Topics: lotus/lamp/set and lotus/lamp/status (prefix configurable).
use std::{env, error::Error, time::Duration};
use lotus_lantern::{Ble, Lamp, LightMode};
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};
use serde::Deserialize;
use serde_json::json;
use tokio::time::sleep;

type AnyError = Box<dyn Error + Send + Sync>;

#[derive(Debug, Deserialize)]
struct Command {
    #[serde(default)] state: Option<String>,
    #[serde(default)] brightness: Option<u8>,
    #[serde(default)] color: Option<Color>,
}
#[derive(Debug, Deserialize)]
struct Color { r: u8, g: u8, b: u8 }

fn parse_command(data: &[u8]) -> Result<Command, AnyError> {
    let text = std::str::from_utf8(data)?.trim();
    if text.eq_ignore_ascii_case("ON") || text == "1" {
        return Ok(Command { state: Some("ON".into()), brightness: None, color: None });
    }
    if text.eq_ignore_ascii_case("OFF") || text == "0" {
        return Ok(Command { state: Some("OFF".into()), brightness: None, color: None });
    }
    let cmd: Command = serde_json::from_str(text)?;
    if let Some(s) = &cmd.state {
        if !s.eq_ignore_ascii_case("ON") && !s.eq_ignore_ascii_case("OFF") {
            return Err("state must be ON or OFF".into());
        }
    }
    if cmd.state.is_none() && cmd.brightness.is_none() && cmd.color.is_none() {
        return Err("command is empty".into());
    }
    Ok(cmd)
}

async fn connect_lamp(ble: &Ble, desired_addr: Option<&str>) -> Result<Lamp, AnyError> {
    let discovered = ble.scan(Duration::from_secs(7)).await?;
    let selected = if let Some(addr) = desired_addr {
        discovered.into_iter().find(|d| d.addr.eq_ignore_ascii_case(addr))
    } else {
        discovered.into_iter().next()
    }.ok_or("lamp not found; verify Bluetooth, power, address and range")?;
    println!("BLE: connecting to {} ({})", selected.name, selected.addr);
    Ok(Lamp::connect(ble, &selected.addr, &selected.name).await?)
}

async fn send_command(ble: &Ble, lamp: &mut Option<Lamp>, addr: Option<&str>, cmd: &Command) -> Result<(), AnyError> {
    let needs_connect = match lamp.as_ref() {
        None => true,
        Some(l) => !l.is_connected().await,
    };
    if needs_connect {
        *lamp = Some(connect_lamp(ble, addr).await?);
    }
    let l = lamp.as_ref().ok_or("BLE connection unavailable")?;
    // Send explicit ON before color / brightness; OFF is always last / exclusive.
    if cmd.state.as_deref().is_some_and(|s| s.eq_ignore_ascii_case("OFF")) {
        l.light_on(false).await?;
        return Ok(());
    }
    if cmd.state.is_some() || cmd.brightness.is_some() || cmd.color.is_some() {
        l.light_on(true).await?;
        sleep(Duration::from_millis(120)).await;
    }
    if let Some(c) = &cmd.color {
        l.set_color_rgb(c.r, c.g, c.b).await?;
        sleep(Duration::from_millis(120)).await;
    }
    if let Some(b) = cmd.brightness {
        l.set_brightness(b, LightMode::Mode0).await?;
    } else if cmd.state.is_some() && cmd.color.is_none() {
        // Some controllers remember brightness=0 even after ON.
        l.set_brightness(80, LightMode::Mode0).await?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), AnyError> {
    let host = env::var("MQTT_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port: u16 = env::var("MQTT_PORT").unwrap_or_else(|_| "1883".into()).parse()?;
    let prefix = env::var("MQTT_TOPIC").unwrap_or_else(|_| "lotus/lamp".into()).trim_end_matches('/').to_owned();
    let address = env::var("LAMP_ADDR").ok();
    let mut opts = MqttOptions::new(format!("lotus-ble-bridge-{}", std::process::id()), host, port);
    opts.set_keep_alive(Duration::from_secs(25));
    if let Ok(username) = env::var("MQTT_USER") {
        opts.set_credentials(username, env::var("MQTT_PASSWORD").unwrap_or_default());
    }
    let (client, mut events) = AsyncClient::new(opts, 32);
    let command_topic = format!("{prefix}/set");
    let status_topic = format!("{prefix}/status");
    let ble = Ble::new().await?;
    let mut lamp: Option<Lamp> = None;
    let mut mqtt_connected = false;
    println!("Listening to {command_topic}; publishing statuses to {status_topic}");
    loop {
        match events.poll().await {
            Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                mqtt_connected = true;
                client.subscribe(&command_topic, QoS::AtLeastOnce).await?;
                client.publish(&status_topic, QoS::AtLeastOnce, false, json!({"status":"ready"}).to_string()).await?;
            }
            Ok(Event::Incoming(Incoming::Publish(p))) if p.topic == command_topic => {
                println!("MQTT received: {}", String::from_utf8_lossy(&p.payload));
                let result = match parse_command(&p.payload) {
                    Ok(cmd) => {
                        let mut result = send_command(&ble, &mut lamp, address.as_deref(), &cmd).await;
                        if result.is_err() {
                            lamp = None;
                            println!("BLE retry after first failure");
                            result = send_command(&ble, &mut lamp, address.as_deref(), &cmd).await;
                        }
                        result
                    },
                    Err(err) => Err(err),
                };
                let message = match result {
                    Ok(()) => json!({"status":"sent","note":"BLE write accepted; physical light state not confirmed"}),
                    Err(err) => { eprintln!("Command failed: {err}"); json!({"status":"error","message":err.to_string()}) }
                };
                if let Err(e) = client.publish(&status_topic, QoS::AtLeastOnce, false, message.to_string()).await {
                    eprintln!("MQTT status publish error: {e}");
                }
            }
            Ok(_) => {}
            Err(err) => {
                if mqtt_connected { eprintln!("MQTT disconnected: {err}"); mqtt_connected = false; }
                sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn parse_on_off() {
        assert_eq!(parse_command(b"ON").unwrap().state.as_deref(), Some("ON"));
        assert_eq!(parse_command(b"off").unwrap().state.as_deref(), Some("OFF"));
    }
    #[test] fn reject_empty() { assert!(parse_command(b"{}").is_err()); }
    #[test] fn parse_json() {
        let c = parse_command(br#"{"state":"ON","brightness":80,"color":{"r":255,"g":0,"b":40}}"#).unwrap();
        assert_eq!(c.brightness, Some(80));
        assert_eq!(c.color.unwrap().b, 40);
    }
}
