//! Medição de boot (spec `desktop-distribution`, requisito "Boot rápido").
//!
//! Com `RTSP_VIEWER_BOOT_TRACE=<arquivo>`, a primeira renderização da janela
//! grava no arquivo o instante em ms desde a época Unix e o tempo desde o
//! início do `main`. Com `RTSP_VIEWER_BOOT_EXIT=1`, o app fecha logo depois,
//! para medições repetidas por script.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub struct BootTrace {
    started: Instant,
    output: Option<PathBuf>,
    exit_after: bool,
}

impl BootTrace {
    pub fn from_env() -> Self {
        Self {
            started: Instant::now(),
            output: std::env::var_os("RTSP_VIEWER_BOOT_TRACE").map(PathBuf::from),
            exit_after: std::env::var("RTSP_VIEWER_BOOT_EXIT").is_ok_and(|v| v == "1"),
        }
    }

    pub fn attach(self, window: &slint::Window) {
        let Some(output) = self.output else {
            return;
        };
        let done = Rc::new(Cell::new(false));
        let started = self.started;
        let exit_after = self.exit_after;
        let result = window.set_rendering_notifier(move |state, _| {
            if !matches!(state, slint::RenderingState::AfterRendering) || done.replace(true) {
                return;
            }
            let epoch_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            let report = format!(
                "first_frame_unix_ms={epoch_ms}\nmain_to_first_frame_ms={}\n",
                started.elapsed().as_millis()
            );
            let _ = std::fs::write(&output, report);
            if exit_after {
                let _ = slint::quit_event_loop();
            }
        });
        if let Err(e) = result {
            let _ = std::fs::write(
                std::env::var_os("RTSP_VIEWER_BOOT_TRACE").unwrap_or_default(),
                format!("erro=notificador de renderização indisponível: {e}\n"),
            );
        }
    }
}
