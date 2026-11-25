//! Diagnóstico: procura câmeras ONVIF na rede e, opcionalmente, consulta os
//! streams de uma delas.
//!
//! ```text
//! cargo run -p rtsp-viewer-onvif --example discover
//! cargo run -p rtsp-viewer-onvif --example discover -- <ip[:porta]> <usuário>
//! ```
//!
//! Com IP e usuário, a senha é lida da variável `ONVIF_PASSWORD` (para não
//! ficar no histórico do terminal) e nunca é impressa.

use rtsp_viewer_core::domain::Password;
use rtsp_viewer_onvif::{Credentials, DiscoveryConfig, OnvifClient, discover};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [target, user, ..] = args.as_slice() {
        let (host, port) = match target.rsplit_once(':') {
            Some((h, p)) if p.parse::<u16>().is_ok() => (h.to_string(), p.parse().ok()),
            _ => (target.clone(), None),
        };
        let password = std::env::var("ONVIF_PASSWORD").unwrap_or_default();
        let creds = Credentials {
            username: user.clone(),
            password: Password::new(password),
        };
        let client = OnvifClient::default();
        let started = std::time::Instant::now();
        let result = client.locate(&host, port).and_then(|d| {
            println!("Serviço ONVIF: {}", d.url);
            client.fetch_streams(&d, Some(&creds))
        });
        match result {
            Ok(set) => {
                println!(
                    "Fabricante: {:?}  Modelo: {:?}",
                    set.manufacturer, set.model
                );
                println!(
                    "Principal: {} ({}x{})",
                    set.main.url, set.main.width, set.main.height
                );
                if let Some(sub) = set.sub {
                    println!("Secundário: {} ({}x{})", sub.url, sub.width, sub.height);
                }
            }
            Err(e) => println!("Falha: {e}"),
        }
        println!("Tempo: {:?}", started.elapsed());
        return;
    }

    let started = std::time::Instant::now();
    match discover(&DiscoveryConfig::default()) {
        Ok(list) if list.is_empty() => println!("Nenhuma câmera encontrada."),
        Ok(list) => {
            for d in list {
                println!(
                    "{}  {}  {}",
                    d.host,
                    d.name.unwrap_or_default(),
                    d.device_url
                );
            }
        }
        Err(e) => println!("Falha: {e}"),
    }
    println!("Tempo: {:?}", started.elapsed());
}
