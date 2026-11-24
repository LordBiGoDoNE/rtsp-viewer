//! Liga as telas Slint ao catálogo de câmeras e ao player.
//!
//! Tudo aqui roda na thread da UI, exceto o sink do player, que só repassa
//! eventos para a thread da UI. Quadros seguem a política "último vence": se a
//! UI estiver ocupada, os anteriores são descartados.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rtsp_viewer_core::application::ports::{
    PlaybackHandle, PlayerEvent, PlayerSink, StreamPlayer, StreamRequest, VideoFrame,
};
use rtsp_viewer_core::application::{CameraCatalog, CameraDraft, CatalogError, PasswordInput};
use rtsp_viewer_core::domain::{CameraId, Password};
use slint::{ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};

use crate::presentation::{self, View};
use crate::{AppWindow, CameraRow, Page, ViewState};

pub struct Controller {
    window: slint::Weak<AppWindow>,
    catalog: RefCell<CameraCatalog>,
    player: Box<dyn StreamPlayer>,
    playback: RefCell<Option<Box<dyn PlaybackHandle>>>,
    /// Sessão de reprodução atual; eventos de sessões anteriores são ignorados.
    session: Arc<AtomicU64>,
    current: RefCell<Option<CameraId>>,
    editing: RefCell<Option<CameraId>>,
}

impl Controller {
    pub fn attach(
        window: &AppWindow,
        catalog: CameraCatalog,
        player: Box<dyn StreamPlayer>,
        banner: Option<String>,
    ) -> Rc<Self> {
        let c = Rc::new(Self {
            window: window.as_weak(),
            catalog: RefCell::new(catalog),
            player,
            playback: RefCell::new(None),
            session: Arc::new(AtomicU64::new(0)),
            current: RefCell::new(None),
            editing: RefCell::new(None),
        });
        window.set_version(env!("CARGO_PKG_VERSION").into());
        if let Some(b) = banner {
            window.set_banner(b.into());
        }
        c.refresh_list();

        macro_rules! on {
            ($setter:ident, |$c:ident $(, $arg:ident)*| $body:expr) => {{
                let $c = c.clone();
                window.$setter(move |$($arg),*| $body);
            }};
        }
        on!(on_add_camera, |c| c.start_form(None));
        on!(on_edit_camera, |c, id| c.start_form(CameraId::parse(&id)));
        on!(on_request_remove, |c, id| c.request_remove(&id));
        on!(on_confirm_remove, |c| c.confirm_remove());
        on!(on_open_camera, |c, id| c.open_camera(&id));
        on!(on_save_form, |c| c.save_form());
        on!(on_cancel_form, |c| c.show_list());
        on!(on_back_to_list, |c| c.back_to_list());
        on!(on_toggle_fullscreen, |c| c.toggle_fullscreen());
        on!(on_retry, |c| c.connect(None));
        on!(on_submit_password, |c| c.submit_password());
        on!(on_cancel_password, |c| c.cancel_password());
        on!(on_show_about, |c| c.go(Page::About));
        on!(on_escape_pressed, |c| c.escape());
        window.invoke_focus_main();
        c
    }

    fn with_window(&self, f: impl FnOnce(&AppWindow)) {
        if let Some(w) = self.window.upgrade() {
            f(&w);
        }
    }

    fn go(&self, page: Page) {
        self.with_window(|w| {
            w.set_page(page);
            w.invoke_focus_main();
        });
    }

    fn refresh_list(&self) {
        let rows: Vec<CameraRow> = self
            .catalog
            .borrow()
            .cameras()
            .iter()
            .map(|c| CameraRow {
                id: c.id().as_str().into(),
                name: c.name().as_str().into(),
                address: c.url().to_string().into(),
            })
            .collect();
        let count = rows.len() as i32;
        self.with_window(|w| {
            w.set_cameras(ModelRc::new(VecModel::from(rows)));
            w.set_selected_index(w.get_selected_index().clamp(0, (count - 1).max(0)));
        });
    }

    fn show_list(&self) {
        *self.editing.borrow_mut() = None;
        self.go(Page::List);
    }

    // ------------------------------------------------------------ formulário

    fn start_form(&self, id: Option<CameraId>) {
        let catalog = self.catalog.borrow();
        let camera = id.as_ref().and_then(|id| catalog.get(id));
        let vault = catalog.passwords_are_persistent();
        self.with_window(|w| {
            w.set_form_editing(camera.is_some());
            w.set_form_title(
                if camera.is_some() {
                    "Editar câmera"
                } else {
                    "Nova câmera"
                }
                .into(),
            );
            w.set_form_name(
                camera
                    .map(|c| c.name().as_str().to_string())
                    .unwrap_or_default()
                    .into(),
            );
            w.set_form_url(
                camera
                    .map(|c| c.url().to_string())
                    .unwrap_or_default()
                    .into(),
            );
            w.set_form_user(camera.and_then(|c| c.username()).unwrap_or_default().into());
            w.set_form_password("".into());
            w.set_name_error("".into());
            w.set_url_error("".into());
            w.set_form_error("".into());
            w.set_vault_available(vault);
        });
        *self.editing.borrow_mut() = camera.map(|c| c.id().clone());
        drop(catalog);
        self.go(Page::Form);
    }

    fn save_form(&self) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        let password = w.get_form_password().to_string();
        let draft = CameraDraft {
            name: w.get_form_name().into(),
            url: w.get_form_url().into(),
            username: w.get_form_user().into(),
            password: if password.is_empty() {
                PasswordInput::Unchanged
            } else {
                PasswordInput::Set(Password::new(password))
            },
        };
        let editing = self.editing.borrow().clone();
        let result = match &editing {
            Some(id) => self.catalog.borrow_mut().edit(id, draft),
            None => self.catalog.borrow_mut().add(draft).map(|_| ()),
        };
        match result {
            Ok(()) => {
                self.refresh_list();
                self.show_list();
            }
            // A câmera foi salva; só a senha falhou. O formulário passa a
            // editar essa câmera, para uma nova tentativa não duplicá-la.
            Err(CatalogError::AddedWithoutPassword { id, reason }) => {
                let message = CatalogError::AddedWithoutPassword {
                    id: id.clone(),
                    reason,
                }
                .to_string();
                *self.editing.borrow_mut() = Some(id);
                self.refresh_list();
                w.set_form_editing(true);
                w.set_form_title("Editar câmera".into());
                w.set_form_error(message.into());
            }
            // Na edição, a lista já foi gravada; só a senha falhou.
            Err(e @ CatalogError::Credentials(_)) => {
                self.refresh_list();
                self.show_list();
                w.set_banner(e.to_string().into());
            }
            Err(e) => {
                let errors = presentation::form_errors(&e);
                w.set_name_error(errors.name.into());
                w.set_url_error(errors.url.into());
                w.set_form_error(errors.general.into());
            }
        }
    }

    // ------------------------------------------------------------ remoção

    fn request_remove(&self, id: &str) {
        let Some(id) = CameraId::parse(id) else {
            return;
        };
        let name = self
            .catalog
            .borrow()
            .get(&id)
            .map(|c| c.name().as_str().to_string());
        if let Some(name) = name {
            self.with_window(|w| {
                w.set_confirm_remove_name(name.into());
                w.set_confirm_remove_id(id.as_str().into());
            });
        }
    }

    fn confirm_remove(&self) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        let id = CameraId::parse(&w.get_confirm_remove_id());
        w.set_confirm_remove_id("".into());
        if let Some(id) = id {
            if let Err(e) = self.catalog.borrow_mut().remove(&id) {
                w.set_banner(e.to_string().into());
            }
            self.refresh_list();
        }
        w.invoke_focus_main();
    }

    // ------------------------------------------------------------ visualização

    fn open_camera(&self, id: &str) {
        let Some(id) = CameraId::parse(id) else {
            return;
        };
        let name = match self.catalog.borrow().get(&id) {
            Some(c) => c.name().as_str().to_string(),
            None => return,
        };
        *self.current.borrow_mut() = Some(id);
        self.with_window(|w| {
            w.set_viewer_name(name.into());
            w.set_video_frame(Image::default());
            w.set_info_text("".into());
            w.set_show_info(false);
            w.set_password_prompt(false);
        });
        self.go(Page::Viewer);
        self.connect(None);
    }

    fn connect(&self, typed: Option<Password>) {
        self.stop_playback();
        let Some(id) = self.current.borrow().clone() else {
            return;
        };
        let request = self.catalog.borrow().connection_request(&id, typed);
        match request {
            Ok(request) => self.start_playback(request),
            Err(CatalogError::NeedsPassword) => {
                let user = self
                    .catalog
                    .borrow()
                    .get(&id)
                    .and_then(|c| c.username().map(str::to_string))
                    .unwrap_or_default();
                self.with_window(|w| {
                    w.set_view_state(ViewState::Connecting);
                    w.set_view_message("Informe a senha para conectar.".into());
                    w.set_password_user(user.into());
                    w.set_prompt_password("".into());
                    w.set_password_prompt(true);
                });
            }
            Err(e) => self.with_window(|w| {
                w.set_view_state(ViewState::Failed);
                w.set_view_message(e.to_string().into());
            }),
        }
    }

    fn start_playback(&self, request: StreamRequest) {
        let session = self.session.fetch_add(1, Ordering::SeqCst) + 1;
        let sink = make_sink(
            self.window.clone(),
            self.session.clone(),
            session,
            request.display_url.clone(),
        );
        self.with_window(|w| {
            w.set_view_state(ViewState::Connecting);
            w.set_view_message("Conectando…".into());
        });
        *self.playback.borrow_mut() = Some(self.player.start(request, sink));
    }

    fn stop_playback(&self) {
        // Invalida eventos atrasados da sessão anterior antes de parar.
        self.session.fetch_add(1, Ordering::SeqCst);
        if let Some(mut handle) = self.playback.borrow_mut().take() {
            handle.stop();
        }
    }

    fn back_to_list(&self) {
        self.stop_playback();
        *self.current.borrow_mut() = None;
        self.with_window(|w| {
            if w.get_fullscreen() {
                w.window().set_fullscreen(false);
                w.set_fullscreen(false);
            }
            w.set_password_prompt(false);
            w.set_video_frame(Image::default());
        });
        self.show_list();
    }

    fn toggle_fullscreen(&self) {
        self.with_window(|w| {
            let on = !w.get_fullscreen();
            w.window().set_fullscreen(on);
            w.set_fullscreen(on);
        });
    }

    fn submit_password(&self) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        let password = Password::new(w.get_prompt_password().to_string());
        w.set_prompt_password("".into());
        w.set_password_prompt(false);
        w.invoke_focus_main();
        self.connect(Some(password));
    }

    fn cancel_password(&self) {
        self.back_to_list();
    }

    /// Esc fecha o que estiver por cima: diálogo, tela cheia, formulário,
    /// visualização ou "Sobre".
    fn escape(&self) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        match w.get_page() {
            Page::List if !w.get_confirm_remove_id().is_empty() => {
                w.set_confirm_remove_id("".into());
            }
            Page::List => {}
            Page::Form | Page::About => self.show_list(),
            Page::Viewer if w.get_password_prompt() => self.cancel_password(),
            Page::Viewer if w.get_fullscreen() => self.toggle_fullscreen(),
            Page::Viewer => self.back_to_list(),
        }
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        if let Some(mut handle) = self.playback.get_mut().take() {
            handle.stop();
        }
    }
}

#[derive(Default)]
struct FrameSlot {
    latest: Mutex<Option<VideoFrame>>,
    pending: AtomicBool,
}

fn to_slint(view: View) -> ViewState {
    match view {
        View::Connecting => ViewState::Connecting,
        View::Playing => ViewState::Playing,
        View::Reconnecting => ViewState::Reconnecting,
        View::Failed => ViewState::Failed,
    }
}

fn frame_image(frame: &VideoFrame) -> Image {
    Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        &frame.rgba,
        frame.width,
        frame.height,
    ))
}

/// Sink chamado pela thread do player: guarda o quadro mais recente e agenda
/// no máximo uma atualização pendente da UI.
fn make_sink(
    window: slint::Weak<AppWindow>,
    current: Arc<AtomicU64>,
    session: u64,
    display_url: String,
) -> PlayerSink {
    let slot = Arc::new(FrameSlot::default());
    Box::new(move |event| {
        let alive = {
            let current = current.clone();
            move || current.load(Ordering::SeqCst) == session
        };
        if !alive() {
            return;
        }
        match event {
            PlayerEvent::Frame(frame) => {
                *slot.latest.lock().unwrap_or_else(|e| e.into_inner()) = Some(frame);
                if !slot.pending.swap(true, Ordering::AcqRel) {
                    let slot = slot.clone();
                    let _ = window.upgrade_in_event_loop(move |w| {
                        slot.pending.store(false, Ordering::Release);
                        let frame = slot.latest.lock().unwrap_or_else(|e| e.into_inner()).take();
                        if let (true, Some(frame)) = (alive(), frame) {
                            w.set_video_frame(frame_image(&frame));
                        }
                    });
                }
            }
            PlayerEvent::State(state) => {
                if let Some((view, message)) = presentation::view_for(&state) {
                    let _ = window.upgrade_in_event_loop(move |w| {
                        if alive() {
                            w.set_view_state(to_slint(view));
                            w.set_view_message(message.into());
                        }
                    });
                }
            }
            PlayerEvent::Info(info) => {
                let text = presentation::info_text(&info, &display_url);
                let _ = window.upgrade_in_event_loop(move |w| {
                    if alive() {
                        w.set_info_text(text.into());
                    }
                });
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use rtsp_viewer_core::application::ports::{PlayerSink, StreamRequest};
    use slint::Model;

    use super::*;
    use crate::infrastructure::credentials::MemoryCredentialStore;
    use crate::infrastructure::memory_repository::MemoryCameraRepository;

    #[derive(Clone, Default)]
    struct FakePlayer {
        requests: Arc<StdMutex<Vec<String>>>,
        stops: Arc<AtomicU64>,
    }

    struct FakeHandle(Arc<AtomicU64>);

    impl PlaybackHandle for FakeHandle {
        fn stop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl StreamPlayer for FakePlayer {
        fn start(&self, request: StreamRequest, _sink: PlayerSink) -> Box<dyn PlaybackHandle> {
            self.requests
                .lock()
                .unwrap()
                .push(request.url.expose().to_string());
            Box::new(FakeHandle(self.stops.clone()))
        }
    }

    fn setup() -> (AppWindow, Rc<Controller>, FakePlayer) {
        i_slint_backend_testing::init_no_event_loop();
        let window = AppWindow::new().unwrap();
        let (catalog, _) = CameraCatalog::load(
            Box::new(MemoryCameraRepository::default()),
            Box::new(MemoryCredentialStore::default()),
        )
        .unwrap();
        let player = FakePlayer::default();
        let c = Controller::attach(&window, catalog, Box::new(player.clone()), None);
        (window, c, player)
    }

    fn add(w: &AppWindow, name: &str, url: &str, user: &str, password: &str) {
        w.invoke_add_camera();
        w.set_form_name(name.into());
        w.set_form_url(url.into());
        w.set_form_user(user.into());
        w.set_form_password(password.into());
        w.invoke_save_form();
    }

    #[test]
    fn adiciona_camera_pelo_formulario() {
        let (w, _c, _) = setup();
        assert_eq!(w.get_page(), Page::List);
        w.invoke_add_camera();
        assert_eq!(w.get_page(), Page::Form);
        assert_eq!(w.get_form_title(), "Nova câmera");
        add(
            &w,
            "Garagem",
            "rtsp://admin:segredo@192.168.0.10/stream1",
            "",
            "",
        );
        assert_eq!(w.get_page(), Page::List);
        let rows = w.get_cameras();
        assert_eq!(rows.row_count(), 1);
        let row = rows.row_data(0).unwrap();
        assert_eq!(row.name, "Garagem");
        assert_eq!(
            row.address, "rtsp://192.168.0.10/stream1",
            "lista nunca mostra a senha"
        );
    }

    #[test]
    fn erros_aparecem_no_campo_e_formulario_continua_aberto() {
        let (w, _c, _) = setup();
        add(&w, "", "http://x", "", "");
        assert_eq!(w.get_page(), Page::Form);
        assert_eq!(w.get_name_error(), "Informe um nome.");
        assert_eq!(
            w.get_url_error(),
            "O endereço deve começar com rtsp:// ou rtsps://."
        );
    }

    #[test]
    fn edita_camera_existente() {
        let (w, _c, _) = setup();
        add(&w, "A", "rtsp://h/s", "admin", "p1");
        let id = w.get_cameras().row_data(0).unwrap().id;
        w.invoke_edit_camera(id);
        assert_eq!(w.get_form_title(), "Editar câmera");
        assert_eq!(w.get_form_user(), "admin");
        assert_eq!(w.get_form_password(), "", "a senha nunca volta para a tela");
        w.set_form_name("B".into());
        w.invoke_save_form();
        assert_eq!(w.get_cameras().row_data(0).unwrap().name, "B");
        assert_eq!(w.get_cameras().row_count(), 1);
    }

    #[test]
    fn remocao_pede_confirmacao() {
        let (w, _c, _) = setup();
        add(&w, "A", "rtsp://h/s", "", "");
        let id = w.get_cameras().row_data(0).unwrap().id;
        w.invoke_request_remove(id);
        assert_eq!(w.get_confirm_remove_name(), "A");
        assert_eq!(w.get_cameras().row_count(), 1);
        w.invoke_confirm_remove();
        assert_eq!(w.get_cameras().row_count(), 0);
        assert_eq!(w.get_confirm_remove_id(), "");
    }

    #[test]
    fn esc_cancela_remocao_sem_remover() {
        let (w, _c, _) = setup();
        add(&w, "A", "rtsp://h/s", "", "");
        let id = w.get_cameras().row_data(0).unwrap().id;
        w.invoke_request_remove(id);
        w.invoke_escape_pressed();
        assert_eq!(w.get_confirm_remove_id(), "");
        assert_eq!(w.get_cameras().row_count(), 1);
    }

    #[test]
    fn abrir_camera_inicia_reproducao_com_credenciais_e_voltar_para() {
        let (w, _c, player) = setup();
        add(&w, "A", "rtsp://h/s", "admin", "p1");
        w.invoke_open_camera(w.get_cameras().row_data(0).unwrap().id);
        assert_eq!(w.get_page(), Page::Viewer);
        assert_eq!(w.get_viewer_name(), "A");
        assert_eq!(w.get_view_message(), "Conectando…");
        assert_eq!(
            player.requests.lock().unwrap().as_slice(),
            ["rtsp://admin:p1@h/s"]
        );
        w.invoke_escape_pressed();
        assert_eq!(w.get_page(), Page::List);
        assert_eq!(player.stops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn sem_senha_pede_ao_usuario_e_conecta() {
        let (w, _c, player) = setup();
        add(&w, "A", "rtsp://h/s", "admin", "");
        w.invoke_open_camera(w.get_cameras().row_data(0).unwrap().id);
        assert!(w.get_password_prompt());
        assert_eq!(w.get_password_user(), "admin");
        assert!(player.requests.lock().unwrap().is_empty());
        w.set_prompt_password("digitada".into());
        w.invoke_submit_password();
        assert!(!w.get_password_prompt());
        assert_eq!(w.get_prompt_password(), "", "campo de senha é limpo");
        assert_eq!(
            player.requests.lock().unwrap().as_slice(),
            ["rtsp://admin:digitada@h/s"]
        );
    }

    #[test]
    fn esc_fecha_formulario_e_sobre() {
        let (w, _c, _) = setup();
        w.invoke_add_camera();
        w.invoke_escape_pressed();
        assert_eq!(w.get_page(), Page::List);
        w.invoke_show_about();
        assert_eq!(w.get_page(), Page::About);
        w.invoke_escape_pressed();
        assert_eq!(w.get_page(), Page::List);
    }

    #[test]
    fn eventos_de_sessao_antiga_sao_ignorados() {
        let current = Arc::new(AtomicU64::new(2));
        let sink = make_sink(slint::Weak::default(), current, 1, "rtsp://h".into());
        // Sem janela e de sessão antiga: não deve entrar em pânico nem agendar nada.
        sink(PlayerEvent::State(
            rtsp_viewer_core::application::ports::PlayerState::Playing,
        ));
    }
}
