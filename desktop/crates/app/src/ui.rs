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
use rtsp_viewer_core::application::{
    CameraCatalog, CameraDraft, CatalogError, PasswordInput, SubstreamInput,
};
use rtsp_viewer_core::domain::{CameraId, Password};
use slint::{ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};

use crate::presentation::{self, View};
use crate::{AppWindow, CameraRow, Page, ViewState};

/// Tarefa executada na thread da UI.
type UiTask = Box<dyn FnOnce(&AppWindow) + Send>;

/// Agenda uma tarefa na thread da UI. No app, `upgrade_in_event_loop`; nos
/// testes, uma fila drenada pelo próprio teste.
type Dispatch = Arc<dyn Fn(UiTask) + Send + Sync>;

fn event_loop_dispatch(window: slint::Weak<AppWindow>) -> Dispatch {
    Arc::new(move |task: UiTask| {
        let _ = window.upgrade_in_event_loop(move |w| task(&w));
    })
}

pub struct Controller {
    window: slint::Weak<AppWindow>,
    dispatch: Dispatch,
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
        let dispatch = event_loop_dispatch(window.as_weak());
        Self::attach_with(window, catalog, player, banner, dispatch)
    }

    fn attach_with(
        window: &AppWindow,
        catalog: CameraCatalog,
        player: Box<dyn StreamPlayer>,
        banner: Option<String>,
        dispatch: Dispatch,
    ) -> Rc<Self> {
        let c = Rc::new(Self {
            window: window.as_weak(),
            dispatch,
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
        on!(on_prepare_playback, |c| c.player.prepare());
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
            // A consulta ONVIF (grupo 4 da descoberta-onvif) preenche o
            // secundário; o formulário de URL completa o mantém enquanto a URL
            // principal não muda (trocá-la descarta o secundário).
            substream: SubstreamInput::Unchanged,
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
            self.dispatch.clone(),
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
    dispatch: Dispatch,
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
                    dispatch(Box::new(move |w| {
                        slot.pending.store(false, Ordering::Release);
                        let frame = slot.latest.lock().unwrap_or_else(|e| e.into_inner()).take();
                        if let (true, Some(frame)) = (alive(), frame) {
                            w.set_video_frame(frame_image(&frame));
                        }
                    }));
                }
            }
            PlayerEvent::State(state) => {
                if let Some((view, message)) = presentation::view_for(&state) {
                    dispatch(Box::new(move |w| {
                        if alive() {
                            w.set_view_state(to_slint(view));
                            w.set_view_message(message.into());
                        }
                    }));
                }
            }
            PlayerEvent::Info(info) => {
                let text = presentation::info_text(&info, &display_url);
                dispatch(Box::new(move |w| {
                    if alive() {
                        w.set_info_text(text.into());
                    }
                }));
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use std::time::Duration;

    use rtsp_viewer_core::application::ports::{
        CredentialError, CredentialStore, DecodeMode, PlayerSink, PlayerState, StreamInfo,
        StreamRequest,
    };
    use slint::Model;

    use super::*;
    use crate::infrastructure::credentials::MemoryCredentialStore;
    use crate::infrastructure::memory_repository::MemoryCameraRepository;

    /// Guarda os sinks recebidos para o teste disparar eventos como o player.
    #[derive(Clone, Default)]
    struct FakePlayer {
        requests: Arc<StdMutex<Vec<String>>>,
        sinks: Arc<StdMutex<Vec<PlayerSink>>>,
        stops: Arc<AtomicU64>,
        prepares: Arc<AtomicU64>,
    }

    impl FakePlayer {
        /// Envia um evento pelo sink da `n`-ésima reprodução iniciada.
        fn emit(&self, n: usize, event: PlayerEvent) {
            (self.sinks.lock().unwrap()[n])(event);
        }
    }

    struct FakeHandle(Arc<AtomicU64>);

    impl PlaybackHandle for FakeHandle {
        fn stop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl StreamPlayer for FakePlayer {
        fn start(&self, request: StreamRequest, sink: PlayerSink) -> Box<dyn PlaybackHandle> {
            self.requests
                .lock()
                .unwrap()
                .push(request.url.expose().to_string());
            self.sinks.lock().unwrap().push(sink);
            Box::new(FakeHandle(self.stops.clone()))
        }

        fn prepare(&self) {
            self.prepares.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Fila no lugar do event loop: o teste decide quando a UI processa.
    #[derive(Clone, Default)]
    struct UiQueue(Arc<StdMutex<Vec<UiTask>>>);

    impl UiQueue {
        fn dispatch(&self) -> Dispatch {
            let queue = self.0.clone();
            Arc::new(move |task| queue.lock().unwrap().push(task))
        }

        fn len(&self) -> usize {
            self.0.lock().unwrap().len()
        }

        fn drain(&self, w: &AppWindow) {
            let tasks = std::mem::take(&mut *self.0.lock().unwrap());
            for task in tasks {
                task(w);
            }
        }
    }

    /// Cofre de teste: persistente e, se `fail`, recusa gravar senhas.
    struct FakeVault {
        fail: bool,
    }

    impl CredentialStore for FakeVault {
        fn get(&self, _: &CameraId) -> Result<Option<Password>, CredentialError> {
            Ok(None)
        }
        fn set(&self, _: &CameraId, _: &Password) -> Result<(), CredentialError> {
            if self.fail {
                Err(CredentialError("cofre bloqueado".into()))
            } else {
                Ok(())
            }
        }
        fn delete(&self, _: &CameraId) -> Result<(), CredentialError> {
            Ok(())
        }
        fn is_persistent(&self) -> bool {
            true
        }
    }

    struct Harness {
        w: AppWindow,
        _c: Rc<Controller>,
        player: FakePlayer,
        ui: UiQueue,
    }

    fn harness(store: Box<dyn CredentialStore>) -> Harness {
        i_slint_backend_testing::init_no_event_loop();
        let w = AppWindow::new().unwrap();
        let (catalog, _) =
            CameraCatalog::load(Box::new(MemoryCameraRepository::default()), store).unwrap();
        let player = FakePlayer::default();
        let ui = UiQueue::default();
        let c = Controller::attach_with(&w, catalog, Box::new(player.clone()), None, ui.dispatch());
        Harness {
            w,
            _c: c,
            player,
            ui,
        }
    }

    fn setup() -> (AppWindow, Rc<Controller>, FakePlayer) {
        let h = harness(Box::new(MemoryCredentialStore::default()));
        (h.w, h._c, h.player)
    }

    /// Janela na visualização de uma câmera "A" em `rtsp://h/s`.
    fn viewing() -> Harness {
        let h = harness(Box::new(MemoryCredentialStore::default()));
        add(&h.w, "A", "rtsp://h/s", "admin", "p1");
        h.w.invoke_open_camera(h.w.get_cameras().row_data(0).unwrap().id);
        assert_eq!(h.w.get_page(), Page::Viewer);
        h
    }

    fn frame(side: u32) -> PlayerEvent {
        PlayerEvent::Frame(VideoFrame {
            width: side,
            height: side,
            rgba: vec![0; (side * side * 4) as usize],
        })
    }

    fn frame_side(w: &AppWindow) -> u32 {
        w.get_video_frame().size().width
    }

    fn press(w: &AppWindow, key: slint::platform::Key) {
        let text: slint::SharedString = key.into();
        w.window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.clone() });
        w.window()
            .dispatch_event(slint::platform::WindowEvent::KeyReleased { text });
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

    /// O dispositivo de hardware não é preparado no boot (custa memória
    /// ociosa); só quando o usuário aponta para uma câmera (design.md, D3).
    #[test]
    fn hardware_so_e_preparado_quando_o_usuario_aponta_para_uma_camera() {
        let (w, _c, player) = setup();
        add(&w, "A", "rtsp://h/a", "", "");
        add(&w, "B", "rtsp://h/b", "", "");
        assert_eq!(w.get_page(), Page::List);
        assert_eq!(
            player.prepares.load(Ordering::SeqCst),
            0,
            "boot não prepara"
        );

        press(&w, slint::platform::Key::DownArrow);
        assert_eq!(player.prepares.load(Ordering::SeqCst), 1, "setas preparam");

        w.window().set_size(slint::LogicalSize::new(1024., 640.));
        // Meio da primeira linha, abaixo da barra superior (medidas dos tokens).
        let tokens: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../design/tokens.json"
        )))
        .unwrap();
        let size = |name: &str| tokens["size"][name].as_f64().unwrap() as f32;
        let (toolbar, row) = (size("toolbar-height"), size("list-item-height"));
        w.window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: slint::LogicalPosition::new(200., toolbar + row / 2.),
            });
        assert_eq!(player.prepares.load(Ordering::SeqCst), 2, "mouse prepara");
        assert!(player.requests.lock().unwrap().is_empty());
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

    // ------------------------------------------------------------ sink do player

    #[test]
    fn ultimo_quadro_vence_com_uma_unica_atualizacao_pendente() {
        let h = viewing();
        h.player.emit(0, frame(2));
        h.player.emit(0, frame(4));
        h.player.emit(0, frame(8));
        assert_eq!(h.ui.len(), 1, "só uma atualização de quadro fica pendente");
        h.ui.drain(&h.w);
        assert_eq!(
            frame_side(&h.w),
            8,
            "o quadro mais recente substitui os anteriores"
        );

        // Depois de processada, o próximo quadro agenda nova atualização.
        h.player.emit(0, frame(6));
        assert_eq!(h.ui.len(), 1);
        h.ui.drain(&h.w);
        assert_eq!(frame_side(&h.w), 6);
    }

    #[test]
    fn estado_e_informacoes_chegam_a_tela() {
        let h = viewing();
        assert_eq!(h.w.get_view_state(), ViewState::Connecting);
        h.player.emit(0, PlayerEvent::State(PlayerState::Playing));
        h.player.emit(
            0,
            PlayerEvent::Info(StreamInfo {
                codec: "h264".into(),
                width: 640,
                height: 480,
                decode: DecodeMode::Software,
            }),
        );
        h.ui.drain(&h.w);
        assert_eq!(h.w.get_view_state(), ViewState::Playing);
        assert_eq!(h.w.get_view_message(), "");
        let info = h.w.get_info_text();
        assert!(info.contains("Resolução: 640x480"), "{info}");
        assert!(info.contains("Endereço: rtsp://h/s"), "{info}");
        assert!(!info.contains("p1"), "a senha nunca aparece: {info}");

        h.player.emit(
            0,
            PlayerEvent::State(PlayerState::Reconnecting {
                attempt: 2,
                delay: Duration::from_secs(2),
            }),
        );
        h.ui.drain(&h.w);
        assert_eq!(h.w.get_view_state(), ViewState::Reconnecting);
        assert_eq!(h.w.get_view_message(), "Reconectando (tentativa 2)…");

        // `Stopped` não muda a tela nem agenda nada.
        h.player.emit(0, PlayerEvent::State(PlayerState::Stopped));
        assert_eq!(h.ui.len(), 0);
    }

    #[test]
    fn tentar_de_novo_reinicia_e_descarta_eventos_da_sessao_antiga() {
        let h = viewing();
        h.player.emit(0, frame(4));
        h.player.emit(0, PlayerEvent::State(PlayerState::Playing));
        assert_eq!(h.ui.len(), 2, "eventos agendados antes do retry");

        h.w.invoke_retry();
        assert_eq!(
            h.player.stops.load(Ordering::SeqCst),
            1,
            "a reprodução anterior para"
        );
        assert_eq!(
            h.player.requests.lock().unwrap().as_slice(),
            ["rtsp://admin:p1@h/s", "rtsp://admin:p1@h/s"],
            "uma nova reprodução começa"
        );
        assert_eq!(h.w.get_view_state(), ViewState::Connecting);

        // Eventos atrasados da sessão antiga nem são agendados...
        h.player.emit(0, frame(8));
        h.player.emit(0, PlayerEvent::State(PlayerState::Playing));
        assert_eq!(h.ui.len(), 2);
        // ...e os já agendados não alteram a tela.
        h.ui.drain(&h.w);
        assert_eq!(frame_side(&h.w), 0, "quadro antigo descartado");
        assert_eq!(h.w.get_view_state(), ViewState::Connecting);

        // A sessão nova continua sendo entregue.
        h.player.emit(1, frame(2));
        h.ui.drain(&h.w);
        assert_eq!(frame_side(&h.w), 2);
    }

    #[test]
    fn voltar_para_lista_invalida_eventos_atrasados() {
        let h = viewing();
        h.player.emit(
            0,
            PlayerEvent::Info(StreamInfo {
                codec: "h264".into(),
                width: 1,
                height: 1,
                decode: DecodeMode::Software,
            }),
        );
        h.player.emit(0, frame(4));
        h.w.invoke_back_to_list();
        assert_eq!(h.w.get_page(), Page::List);
        assert_eq!(h.player.stops.load(Ordering::SeqCst), 1);
        h.player.emit(0, frame(8));
        assert_eq!(h.ui.len(), 2, "evento após parar não é agendado");
        h.ui.drain(&h.w);
        assert_eq!(h.w.get_info_text(), "");
        assert_eq!(frame_side(&h.w), 0);
    }

    // ------------------------------------------------------------ tela cheia

    #[test]
    fn f11_e_botao_alternam_tela_cheia() {
        let h = viewing();
        assert!(!h.w.get_fullscreen());
        press(&h.w, slint::platform::Key::F11);
        assert!(h.w.get_fullscreen(), "F11 entra em tela cheia");
        press(&h.w, slint::platform::Key::F11);
        assert!(!h.w.get_fullscreen(), "F11 sai da tela cheia");
        h.w.invoke_toggle_fullscreen();
        assert!(h.w.get_fullscreen());
        h.w.invoke_toggle_fullscreen();
        assert!(!h.w.get_fullscreen());
    }

    #[test]
    fn f11_fora_da_visualizacao_nao_faz_nada() {
        let (w, _c, _) = setup();
        press(&w, slint::platform::Key::F11);
        assert!(!w.get_fullscreen());
    }

    #[test]
    fn esc_sai_da_tela_cheia_e_segundo_esc_volta_a_lista() {
        let h = viewing();
        press(&h.w, slint::platform::Key::F11);
        assert!(h.w.get_fullscreen());
        press(&h.w, slint::platform::Key::Escape);
        assert!(!h.w.get_fullscreen(), "primeiro Esc sai da tela cheia");
        assert_eq!(h.w.get_page(), Page::Viewer);
        assert_eq!(h.player.stops.load(Ordering::SeqCst), 0);
        press(&h.w, slint::platform::Key::Escape);
        assert_eq!(h.w.get_page(), Page::List, "segundo Esc volta à lista");
        assert_eq!(h.player.stops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn voltar_em_tela_cheia_sai_da_tela_cheia() {
        let h = viewing();
        h.w.invoke_toggle_fullscreen();
        h.w.invoke_back_to_list();
        assert!(!h.w.get_fullscreen());
        assert_eq!(h.w.get_page(), Page::List);
    }

    // ------------------------------------------------------------ cofre

    #[test]
    fn formulario_avisa_quando_nao_ha_cofre() {
        let (w, _c, _) = setup();
        w.invoke_add_camera();
        assert!(
            !w.get_vault_available(),
            "senhas só em memória: mostra o aviso"
        );
    }

    #[test]
    fn formulario_sem_aviso_quando_ha_cofre() {
        let h = harness(Box::new(FakeVault { fail: false }));
        h.w.invoke_add_camera();
        assert!(h.w.get_vault_available(), "com cofre: sem aviso");
    }

    #[test]
    fn falha_no_cofre_ao_adicionar_nao_duplica_a_camera() {
        let h = harness(Box::new(FakeVault { fail: true }));
        add(&h.w, "A", "rtsp://h/s", "admin", "p1");
        assert_eq!(h.w.get_page(), Page::Form, "formulário continua aberto");
        assert_eq!(h.w.get_cameras().row_count(), 1, "a câmera foi salva");
        assert!(
            h.w.get_form_editing(),
            "o formulário passa a editar a câmera"
        );
        assert_eq!(h.w.get_form_title(), "Editar câmera");
        let error = h.w.get_form_error();
        assert!(!error.is_empty());
        assert!(
            !error.contains("p1"),
            "a senha não aparece no erro: {error}"
        );

        // Nova tentativa edita a mesma câmera em vez de criar outra.
        h.w.set_form_password("p1".into());
        h.w.invoke_save_form();
        assert_eq!(h.w.get_cameras().row_count(), 1);
        assert_eq!(h.w.get_page(), Page::List);
        assert!(!h.w.get_banner().is_empty(), "falha da senha vira aviso");
    }
}
