//! Liga as telas Slint ao catálogo de câmeras, ao player e às portas ONVIF.
//!
//! Tudo aqui roda na thread da UI, exceto o sink do player e os trabalhos de
//! rede (busca e consulta ONVIF), que rodam em threads próprias e só repassam
//! o resultado para a thread da UI (design D7). Quadros seguem a política
//! "último vence": se a UI estiver ocupada, os anteriores são descartados.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rtsp_viewer_core::application::ports::{
    CameraDiscovery, DiscoveredCamera, DiscoveryFailure, PlaybackHandle, PlayerEvent, PlayerSink,
    StreamPlayer, StreamRequest, StreamResolver, StreamTarget, VideoFrame,
};
use rtsp_viewer_core::application::{
    AddressInput, CameraCatalog, CameraDraft, CatalogError, LookupError, LookupOutcome,
    PasswordInput, SubstreamInput, classify_address, lookup, vendor_streams,
};
use rtsp_viewer_core::domain::{CameraId, CameraName, DeviceAddress, Password, Vendor};
use slint::{
    ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel,
};

use crate::presentation::{self, View};
use crate::{AppWindow, CameraRow, DiscoverState, FormState, FoundRow, Page, ViewState};

/// Tarefa executada na thread da UI.
type UiTask = Box<dyn FnOnce(&AppWindow) + Send>;

/// Agenda uma tarefa na thread da UI. No app, `upgrade_in_event_loop`; nos
/// testes, uma fila drenada pelo próprio teste.
type Dispatch = Arc<dyn Fn(UiTask) + Send + Sync>;

/// Trabalho bloqueante (rede), executado fora da thread da UI.
type Job = Box<dyn FnOnce() + Send>;

/// Inicia um trabalho em segundo plano. No app, uma thread nova; nos testes,
/// uma fila executada pelo próprio teste.
type Spawn = Box<dyn Fn(&'static str, Job) -> Result<(), String>>;

/// Resultado de um trabalho em segundo plano, com a sessão que o pediu. É
/// preenchido na thread da UI, logo antes do callback que o consome.
type Slot<T> = Arc<Mutex<Option<(u64, T)>>>;

type DiscoveryResult = Result<Vec<DiscoveredCamera>, DiscoveryFailure>;
type LookupResult = Result<LookupOutcome, LookupError>;

fn event_loop_dispatch(window: slint::Weak<AppWindow>) -> Dispatch {
    Arc::new(move |task: UiTask| {
        let _ = window.upgrade_in_event_loop(move |w| task(&w));
    })
}

fn thread_spawn() -> Spawn {
    Box::new(|name, job| {
        std::thread::Builder::new()
            .name(name.into())
            .spawn(job)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
}

/// Adaptadores usados pela UI.
pub struct Services {
    pub player: Box<dyn StreamPlayer>,
    pub resolver: Arc<dyn StreamResolver>,
    pub discovery: Arc<dyn CameraDiscovery>,
}

/// Formulário à espera da consulta ONVIF (ou da escolha de fabricante).
struct PendingLookup {
    name: String,
    address: DeviceAddress,
    username: String,
    password: Option<Password>,
}

pub struct Controller {
    window: slint::Weak<AppWindow>,
    dispatch: Dispatch,
    spawn: Spawn,
    catalog: RefCell<CameraCatalog>,
    player: Box<dyn StreamPlayer>,
    resolver: Arc<dyn StreamResolver>,
    discovery: Arc<dyn CameraDiscovery>,
    playback: RefCell<Option<Box<dyn PlaybackHandle>>>,
    /// Sessão de reprodução atual; eventos de sessões anteriores são ignorados.
    session: Arc<AtomicU64>,
    current: RefCell<Option<CameraId>>,
    editing: RefCell<Option<CameraId>>,
    /// Sessão da busca atual; resultado de busca abandonada é ignorado.
    discovery_session: Arc<AtomicU64>,
    discovery_slot: Slot<DiscoveryResult>,
    found: RefCell<Vec<DiscoveredCamera>>,
    /// Sessão da consulta atual; resultado de consulta cancelada é ignorado.
    lookup_session: Arc<AtomicU64>,
    lookup_slot: Slot<LookupResult>,
    pending: RefCell<Option<PendingLookup>>,
    /// Câmera escolhida na busca: (endereço posto no formulário, URL do
    /// serviço ONVIF). Vale enquanto o endereço não for alterado.
    found_target: RefCell<Option<(String, String)>>,
}

impl Controller {
    pub fn attach(
        window: &AppWindow,
        catalog: CameraCatalog,
        services: Services,
        banner: Option<String>,
    ) -> Rc<Self> {
        let dispatch = event_loop_dispatch(window.as_weak());
        Self::attach_with(window, catalog, services, banner, dispatch, thread_spawn())
    }

    fn attach_with(
        window: &AppWindow,
        catalog: CameraCatalog,
        services: Services,
        banner: Option<String>,
        dispatch: Dispatch,
        spawn: Spawn,
    ) -> Rc<Self> {
        let c = Rc::new(Self {
            window: window.as_weak(),
            dispatch,
            spawn,
            catalog: RefCell::new(catalog),
            player: services.player,
            resolver: services.resolver,
            discovery: services.discovery,
            playback: RefCell::new(None),
            session: Arc::new(AtomicU64::new(0)),
            current: RefCell::new(None),
            editing: RefCell::new(None),
            discovery_session: Arc::new(AtomicU64::new(0)),
            discovery_slot: Arc::default(),
            found: RefCell::new(Vec::new()),
            lookup_session: Arc::new(AtomicU64::new(0)),
            lookup_slot: Arc::default(),
            pending: RefCell::new(None),
            found_target: RefCell::new(None),
        });
        window.set_version(env!("CARGO_PKG_VERSION").into());
        if let Some(b) = banner {
            window.set_banner(b.into());
        }
        let vendors: Vec<SharedString> = Vendor::ALL.iter().map(|v| v.label().into()).collect();
        window.set_vendors(ModelRc::new(VecModel::from(vendors)));
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
        on!(on_cancel_lookup, |c| c.cancel_lookup());
        on!(on_choose_vendor, |c, i| c.choose_vendor(i));
        on!(on_cancel_vendor, |c| c.cancel_lookup());
        on!(on_lookup_ready, |c| c.lookup_ready());
        on!(on_discover_cameras, |c| c.open_discovery());
        on!(on_rediscover, |c| c.start_discovery());
        on!(on_close_discover, |c| c.close_discovery());
        on!(on_add_found, |c, i| c.add_found(i));
        on!(on_discovery_ready, |c| c.discovery_ready());
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
        self.abandon_lookup();
        self.go(Page::List);
    }

    /// Roda `work` numa thread e entrega o resultado na thread da UI pelo
    /// `slot` e pelo callback `ready`, marcado com a sessão `id`. Se a sessão
    /// mudar antes do fim (cancelada ou substituída), nada é agendado.
    fn run_in_background<T: Send + 'static>(
        &self,
        name: &'static str,
        session: &Arc<AtomicU64>,
        id: u64,
        slot: &Slot<T>,
        ready: fn(&AppWindow),
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Result<(), String> {
        let session = session.clone();
        let slot = slot.clone();
        let dispatch = self.dispatch.clone();
        (self.spawn)(
            name,
            Box::new(move || {
                let result = work();
                if session.load(Ordering::SeqCst) != id {
                    return;
                }
                dispatch(Box::new(move |w| {
                    *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some((id, result));
                    ready(w);
                }));
            }),
        )
    }

    // ------------------------------------------------------------ formulário

    fn start_form(&self, id: Option<CameraId>) {
        self.leave_discovery();
        self.abandon_lookup();
        *self.found_target.borrow_mut() = None;
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
        // Enter num campo durante a consulta não inicia outra.
        if w.get_form_state() != FormState::Editing {
            return;
        }
        let url = w.get_form_url().to_string();
        match classify_address(&url) {
            Ok(AddressInput::Rtsp) => {
                let password = w.get_form_password().to_string();
                self.save_draft(
                    &w,
                    CameraDraft {
                        name: w.get_form_name().into(),
                        url,
                        username: w.get_form_user().into(),
                        password: if password.is_empty() {
                            PasswordInput::Unchanged
                        } else {
                            PasswordInput::Set(Password::new(password))
                        },
                        // URL completa: o secundário atual é mantido enquanto
                        // a URL principal não muda (trocá-la o descarta).
                        substream: SubstreamInput::Unchanged,
                    },
                );
            }
            Ok(AddressInput::Device(address)) => self.start_lookup(&w, &url, address),
            Err(e) => {
                let name = w.get_form_name();
                w.set_name_error(
                    CameraName::parse(&name)
                        .err()
                        .map(|e| e.to_string())
                        .unwrap_or_default()
                        .into(),
                );
                w.set_url_error(e.to_string().into());
                w.set_form_error("".into());
            }
        }
    }

    /// Grava o rascunho (adiciona ou edita) e trata o resultado na tela.
    fn save_draft(&self, w: &AppWindow, draft: CameraDraft) {
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

    // ------------------------------------------------------------ consulta ONVIF

    /// Endereço sem caminho RTSP: consulta a câmera em segundo plano.
    fn start_lookup(&self, w: &AppWindow, url: &str, address: DeviceAddress) {
        let name = w.get_form_name().trim().to_string();
        let name_error = if name.is_empty() {
            String::new()
        } else {
            CameraName::parse(&name)
                .err()
                .map(|e| e.to_string())
                .unwrap_or_default()
        };
        w.set_name_error(name_error.clone().into());
        w.set_url_error("".into());
        w.set_form_error("".into());
        if !name_error.is_empty() {
            return;
        }
        let username = w.get_form_user().to_string();
        let password = w.get_form_password().to_string();
        if self.editing.borrow().is_some() && !username.trim().is_empty() && password.is_empty() {
            w.set_form_error(presentation::LOOKUP_NEEDS_PASSWORD.into());
            return;
        }
        let target = match &*self.found_target.borrow() {
            Some((text, service)) if text.trim() == url.trim() => {
                StreamTarget::ServiceUrl(service.clone())
            }
            _ => StreamTarget::Address(address.clone()),
        };
        let password = (!password.is_empty()).then(|| Password::new(password));
        *self.pending.borrow_mut() = Some(PendingLookup {
            name,
            address,
            username: username.clone(),
            password: password.clone(),
        });
        let id = self.lookup_session.fetch_add(1, Ordering::SeqCst) + 1;
        w.set_form_state(FormState::LookingUp);
        let resolver = self.resolver.clone();
        let started = self.run_in_background(
            "onvif-consulta",
            &self.lookup_session,
            id,
            &self.lookup_slot,
            |w| w.invoke_lookup_ready(),
            move || lookup(&*resolver, &target, Some(&username), password.as_ref()),
        );
        if let Err(e) = started {
            self.abandon_lookup();
            w.set_form_error(format!("Não foi possível consultar a câmera: {e}").into());
        }
    }

    fn lookup_ready(&self) {
        let Some(result) = take_current(&self.lookup_slot, &self.lookup_session) else {
            return;
        };
        let Some(pending) = self.pending.borrow_mut().take() else {
            return;
        };
        let Some(w) = self.window.upgrade() else {
            return;
        };
        // A consulta terminou: a senha sai da tela (fica só no rascunho).
        w.set_form_password("".into());
        w.set_form_state(FormState::Editing);
        match result {
            Ok(LookupOutcome::Resolved(streams)) => {
                let name = if pending.name.is_empty() {
                    streams
                        .suggested_name
                        .clone()
                        .unwrap_or_else(|| presentation::fallback_name(&pending.address))
                } else {
                    pending.name.clone()
                };
                let sub = streams.sub.map(|s| s.to_string());
                self.save_resolved(&w, pending, name, streams.main.to_string(), sub);
            }
            Ok(LookupOutcome::NeedsVendor { reason }) => {
                w.set_vendor_reason(presentation::vendor_reason(&reason).into());
                w.set_form_state(FormState::ChooseVendor);
                *self.pending.borrow_mut() = Some(pending);
            }
            Err(e @ LookupError::AuthenticationFailed) => {
                w.set_form_error(e.to_string().into());
            }
        }
        w.invoke_focus_main();
    }

    fn choose_vendor(&self, index: i32) {
        let Some(vendor) = usize::try_from(index)
            .ok()
            .and_then(|i| Vendor::ALL.get(i).copied())
        else {
            return;
        };
        let Some(pending) = self.pending.borrow_mut().take() else {
            return;
        };
        let Some(w) = self.window.upgrade() else {
            return;
        };
        w.set_form_state(FormState::Editing);
        let (main, sub) = vendor_streams(vendor, &pending.address);
        let name = if pending.name.is_empty() {
            presentation::fallback_name(&pending.address)
        } else {
            pending.name.clone()
        };
        let sub = sub.map(|s| s.to_string());
        self.save_resolved(&w, pending, name, main.to_string(), sub);
    }

    fn save_resolved(
        &self,
        w: &AppWindow,
        pending: PendingLookup,
        name: String,
        main: String,
        sub: Option<String>,
    ) {
        self.save_draft(
            w,
            CameraDraft {
                name,
                url: main,
                username: pending.username,
                password: pending
                    .password
                    .map(PasswordInput::Set)
                    .unwrap_or(PasswordInput::Unchanged),
                substream: sub
                    .map(SubstreamInput::Set)
                    .unwrap_or(SubstreamInput::Clear),
            },
        );
    }

    /// Cancela a consulta (ou a escolha de fabricante) e volta ao formulário.
    fn cancel_lookup(&self) {
        self.abandon_lookup();
        self.with_window(|w| w.invoke_focus_main());
    }

    /// Invalida a consulta em andamento: o resultado que chegar é ignorado.
    fn abandon_lookup(&self) {
        self.lookup_session.fetch_add(1, Ordering::SeqCst);
        *self.pending.borrow_mut() = None;
        self.with_window(|w| {
            w.set_form_state(FormState::Editing);
            w.set_vendor_reason("".into());
        });
    }

    // ------------------------------------------------------------ busca na rede

    fn open_discovery(&self) {
        self.go(Page::Discover);
        self.start_discovery();
    }

    /// A busca só roda por ação do usuário (nunca no boot).
    fn start_discovery(&self) {
        let id = self.discovery_session.fetch_add(1, Ordering::SeqCst) + 1;
        self.found.borrow_mut().clear();
        self.with_window(|w| {
            w.set_discover_state(DiscoverState::Searching);
            w.set_found(ModelRc::default());
            w.set_discover_error("".into());
        });
        let discovery = self.discovery.clone();
        let started = self.run_in_background(
            "onvif-busca",
            &self.discovery_session,
            id,
            &self.discovery_slot,
            |w| w.invoke_discovery_ready(),
            move || discovery.discover(),
        );
        if let Err(e) = started {
            self.show_found(Err(DiscoveryFailure(e)));
        }
    }

    fn discovery_ready(&self) {
        if let Some(result) = take_current(&self.discovery_slot, &self.discovery_session) {
            self.show_found(result);
        }
    }

    fn show_found(&self, result: DiscoveryResult) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        match result {
            Ok(list) => {
                let rows: Vec<FoundRow> = list
                    .iter()
                    .map(|d| FoundRow {
                        name: presentation::found_name(d.suggested_name.as_deref()).into(),
                        host: d.host.as_str().into(),
                        registered: self.is_registered(&d.host),
                    })
                    .collect();
                w.set_discover_state(if rows.is_empty() {
                    DiscoverState::Empty
                } else {
                    DiscoverState::Found
                });
                w.set_found(ModelRc::new(VecModel::from(rows)));
                *self.found.borrow_mut() = list;
            }
            Err(e) => {
                w.set_discover_error(presentation::discovery_error(&e).into());
                w.set_discover_state(DiscoverState::Failed);
            }
        }
    }

    /// Já existe câmera com este host na URL principal.
    fn is_registered(&self, host: &str) -> bool {
        let host = host.trim_matches(['[', ']']);
        self.catalog
            .borrow()
            .cameras()
            .iter()
            .any(|c| c.url().host().eq_ignore_ascii_case(host))
    }

    /// "Adicionar" de uma câmera encontrada: formulário com o nome sugerido e
    /// o host; a consulta usa a URL de serviço anunciada.
    fn add_found(&self, index: i32) {
        let Some(camera) = usize::try_from(index)
            .ok()
            .and_then(|i| self.found.borrow().get(i).cloned())
        else {
            return;
        };
        self.start_form(None);
        let address = if camera.host.contains(':') && !camera.host.starts_with('[') {
            format!("[{}]", camera.host)
        } else {
            camera.host.clone()
        };
        self.with_window(|w| {
            w.set_form_name(camera.suggested_name.clone().unwrap_or_default().into());
            w.set_form_url(address.as_str().into());
        });
        *self.found_target.borrow_mut() = Some((address, camera.device_url));
    }

    fn close_discovery(&self) {
        self.leave_discovery();
        self.show_list();
    }

    /// Invalida a busca em andamento: o resultado que chegar é ignorado.
    fn leave_discovery(&self) {
        self.discovery_session.fetch_add(1, Ordering::SeqCst);
        self.found.borrow_mut().clear();
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

    /// Esc fecha o que estiver por cima: diálogo, consulta, escolha de
    /// fabricante, tela cheia, formulário, busca, visualização ou "Sobre".
    fn escape(&self) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        match w.get_page() {
            Page::List if !w.get_confirm_remove_id().is_empty() => {
                w.set_confirm_remove_id("".into());
            }
            Page::List => {}
            Page::Form if w.get_form_state() != FormState::Editing => self.cancel_lookup(),
            Page::Form | Page::About => self.show_list(),
            Page::Discover => self.close_discovery(),
            Page::Viewer if w.get_password_prompt() => self.cancel_password(),
            Page::Viewer if w.get_fullscreen() => self.toggle_fullscreen(),
            Page::Viewer => self.back_to_list(),
        }
    }
}

/// Tira do `slot` o resultado, se ele for da sessão atual.
fn take_current<T>(slot: &Slot<T>, session: &AtomicU64) -> Option<T> {
    let (id, value) = slot.lock().unwrap_or_else(|e| e.into_inner()).take()?;
    (id == session.load(Ordering::SeqCst)).then_some(value)
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
        CredentialError, CredentialStore, DecodeMode, PlayerSink, PlayerState, ResolveError,
        ResolvedStreams, StreamInfo, StreamRequest,
    };
    use rtsp_viewer_core::domain::RtspUrl;
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

    /// Fila no lugar das threads: o teste decide quando (e em que ordem) os
    /// trabalhos de rede terminam.
    #[derive(Clone, Default)]
    struct Jobs(Arc<StdMutex<Vec<Option<Job>>>>);

    impl Jobs {
        fn spawn(&self) -> Spawn {
            let jobs = self.0.clone();
            Box::new(move |_, job| {
                jobs.lock().unwrap().push(Some(job));
                Ok(())
            })
        }

        fn count(&self) -> usize {
            self.0.lock().unwrap().len()
        }

        /// Executa o `n`-ésimo trabalho iniciado.
        fn run(&self, n: usize) {
            let job = self.0.lock().unwrap()[n]
                .take()
                .expect("trabalho já executado");
            job();
        }
    }

    /// Chamada registrada: alvo, usuário e se houve senha.
    type ResolveCall = (StreamTarget, Option<String>, bool);

    /// Consulta ONVIF falsa: devolve o resultado programado e registra as
    /// chamadas.
    #[derive(Clone)]
    struct FakeResolver {
        result: Arc<StdMutex<Result<ResolvedStreams, ResolveError>>>,
        calls: Arc<StdMutex<Vec<ResolveCall>>>,
    }

    impl Default for FakeResolver {
        fn default() -> Self {
            Self {
                result: Arc::new(StdMutex::new(Err(ResolveError::NotOnvif))),
                calls: Arc::default(),
            }
        }
    }

    impl FakeResolver {
        fn returns(&self, result: Result<ResolvedStreams, ResolveError>) {
            *self.result.lock().unwrap() = result;
        }
    }

    impl StreamResolver for FakeResolver {
        fn resolve(
            &self,
            target: &StreamTarget,
            username: Option<&str>,
            password: Option<&Password>,
        ) -> Result<ResolvedStreams, ResolveError> {
            self.calls.lock().unwrap().push((
                target.clone(),
                username.map(str::to_string),
                password.is_some(),
            ));
            self.result.lock().unwrap().clone()
        }
    }

    /// Busca falsa: cada chamada devolve o próximo resultado programado
    /// (lista vazia quando acabam).
    #[derive(Clone, Default)]
    struct FakeDiscovery {
        results: Arc<StdMutex<Vec<DiscoveryResult>>>,
        calls: Arc<AtomicU64>,
    }

    impl FakeDiscovery {
        fn then(&self, result: DiscoveryResult) {
            self.results.lock().unwrap().push(result);
        }
    }

    impl CameraDiscovery for FakeDiscovery {
        fn discover(&self) -> DiscoveryResult {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let mut results = self.results.lock().unwrap();
            if results.is_empty() {
                Ok(vec![])
            } else {
                results.remove(0)
            }
        }
    }

    struct Harness {
        w: AppWindow,
        _c: Rc<Controller>,
        player: FakePlayer,
        ui: UiQueue,
        jobs: Jobs,
        resolver: FakeResolver,
        discovery: FakeDiscovery,
    }

    impl Harness {
        /// Termina o `n`-ésimo trabalho e processa a fila da UI.
        fn finish(&self, n: usize) {
            self.jobs.run(n);
            self.ui.drain(&self.w);
        }

        fn catalog(&self) -> std::cell::Ref<'_, CameraCatalog> {
            self._c.catalog.borrow()
        }
    }

    fn harness(store: Box<dyn CredentialStore>) -> Harness {
        i_slint_backend_testing::init_no_event_loop();
        let w = AppWindow::new().unwrap();
        let (catalog, _) =
            CameraCatalog::load(Box::new(MemoryCameraRepository::default()), store).unwrap();
        let player = FakePlayer::default();
        let ui = UiQueue::default();
        let jobs = Jobs::default();
        let resolver = FakeResolver::default();
        let discovery = FakeDiscovery::default();
        let services = Services {
            player: Box::new(player.clone()),
            resolver: Arc::new(resolver.clone()),
            discovery: Arc::new(discovery.clone()),
        };
        let c = Controller::attach_with(&w, catalog, services, None, ui.dispatch(), jobs.spawn());
        Harness {
            w,
            _c: c,
            player,
            ui,
            jobs,
            resolver,
            discovery,
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
            "Use um IP, um nome de host ou uma URL rtsp:// completa."
        );
        add(&w, "A", "192.168.1.164:abc", "", "");
        assert_eq!(w.get_name_error(), "");
        assert_eq!(
            w.get_url_error(),
            "Porta inválida: use um número entre 1 e 65535."
        );
        add(&w, "A", "rtsp://", "", "");
        assert!(
            !w.get_url_error().is_empty(),
            "URL RTSP sem host é recusada"
        );
        assert_eq!(w.get_cameras().row_count(), 0);
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

    // ------------------------------------------------------------ busca na rede

    fn found(host: &str, name: Option<&str>) -> DiscoveredCamera {
        DiscoveredCamera {
            host: host.into(),
            device_url: format!("http://{host}:2020/onvif/device_service"),
            suggested_name: name.map(str::to_string),
        }
    }

    fn found_rows(w: &AppWindow) -> Vec<(String, String, bool)> {
        w.get_found()
            .iter()
            .map(|r| (r.name.to_string(), r.host.to_string(), r.registered))
            .collect()
    }

    #[test]
    fn boot_nao_procura_cameras() {
        let h = harness(Box::new(MemoryCredentialStore::default()));
        assert_eq!(h.discovery.calls.load(Ordering::SeqCst), 0);
        assert_eq!(h.jobs.count(), 0, "nada de rede no boot");
    }

    #[test]
    fn busca_lista_cameras_e_marca_ja_cadastrada() {
        let h = harness(Box::new(MemoryCredentialStore::default()));
        add(&h.w, "Portão", "rtsp://192.168.1.50:554/stream1", "", "");
        h.discovery.then(Ok(vec![
            found("192.168.1.50", Some("Tapo C200")),
            found("192.168.1.60", None),
        ]));

        h.w.invoke_discover_cameras();
        assert_eq!(h.w.get_page(), Page::Discover);
        assert_eq!(h.w.get_discover_state(), DiscoverState::Searching);
        assert_eq!(h.jobs.count(), 1, "a busca roda fora da thread da UI");
        assert_eq!(h.discovery.calls.load(Ordering::SeqCst), 0);

        h.finish(0);
        assert_eq!(h.w.get_discover_state(), DiscoverState::Found);
        assert_eq!(
            found_rows(&h.w),
            [
                ("Tapo C200".into(), "192.168.1.50".into(), true),
                ("Câmera ONVIF".into(), "192.168.1.60".into(), false),
            ]
        );

        // "Procurar de novo" refaz a busca.
        h.discovery.then(Ok(vec![found("192.168.1.70", None)]));
        h.w.invoke_rediscover();
        assert_eq!(h.w.get_discover_state(), DiscoverState::Searching);
        assert_eq!(h.w.get_found().row_count(), 0);
        h.finish(1);
        assert_eq!(found_rows(&h.w)[0].1, "192.168.1.70");
    }

    #[test]
    fn busca_vazia_e_busca_com_erro() {
        let h = harness(Box::new(MemoryCredentialStore::default()));
        h.w.invoke_discover_cameras();
        h.finish(0);
        assert_eq!(h.w.get_discover_state(), DiscoverState::Empty);

        h.discovery
            .then(Err(DiscoveryFailure("nenhuma interface de rede".into())));
        h.w.invoke_rediscover();
        h.finish(1);
        assert_eq!(h.w.get_discover_state(), DiscoverState::Failed);
        assert_eq!(
            h.w.get_discover_error(),
            "Não foi possível procurar câmeras: nenhuma interface de rede"
        );

        // "Cadastrar pelo IP" abre o formulário vazio.
        h.w.invoke_add_camera();
        assert_eq!(h.w.get_page(), Page::Form);
        assert_eq!(h.w.get_form_url(), "");
    }

    #[test]
    fn resultado_de_busca_antiga_e_ignorado() {
        let h = harness(Box::new(MemoryCredentialStore::default()));
        h.discovery
            .then(Ok(vec![found("10.0.0.1", Some("Antiga"))]));
        h.discovery.then(Ok(vec![found("10.0.0.2", Some("Nova"))]));
        h.w.invoke_discover_cameras();
        h.jobs.run(0);
        assert_eq!(
            h.ui.len(),
            1,
            "resultado agendado antes de procurar de novo"
        );
        h.w.invoke_rediscover();
        h.ui.drain(&h.w);
        assert_eq!(
            h.w.get_discover_state(),
            DiscoverState::Searching,
            "resultado da busca anterior não muda a tela"
        );
        h.finish(1);
        assert_eq!(
            found_rows(&h.w),
            [("Nova".into(), "10.0.0.2".into(), false)]
        );
    }

    #[test]
    fn esc_fecha_a_busca_e_resultado_atrasado_e_ignorado() {
        let h = harness(Box::new(MemoryCredentialStore::default()));
        h.discovery.then(Ok(vec![found("10.0.0.1", None)]));
        h.w.invoke_discover_cameras();
        h.w.invoke_escape_pressed();
        assert_eq!(h.w.get_page(), Page::List);
        h.finish(0);
        assert_eq!(h.ui.len(), 0);
        assert_eq!(
            h.w.get_page(),
            Page::List,
            "busca abandonada não reabre a tela"
        );
        assert_eq!(h.w.get_found().row_count(), 0);

        h.w.invoke_discover_cameras();
        h.w.invoke_close_discover();
        assert_eq!(h.w.get_page(), Page::List, "\"Voltar\" também fecha");
    }

    #[test]
    fn adicionar_encontrada_consulta_pela_url_de_servico() {
        let h = harness(Box::new(MemoryCredentialStore::default()));
        h.discovery
            .then(Ok(vec![found("192.168.1.60", Some("Tapo C200"))]));
        h.w.invoke_discover_cameras();
        h.finish(0);
        h.w.invoke_add_found(0);
        assert_eq!(h.w.get_page(), Page::Form);
        assert_eq!(h.w.get_form_title(), "Nova câmera");
        assert_eq!(h.w.get_form_name(), "Tapo C200");
        assert_eq!(h.w.get_form_url(), "192.168.1.60");

        h.resolver.returns(Ok(streams("192.168.1.60", None)));
        h.w.set_form_user("admin".into());
        h.w.set_form_password("segredo".into());
        h.w.invoke_save_form();
        h.finish(1);
        assert_eq!(
            h.resolver.calls.lock().unwrap().as_slice(),
            [(
                StreamTarget::ServiceUrl("http://192.168.1.60:2020/onvif/device_service".into()),
                Some("admin".into()),
                true
            )]
        );
        assert_eq!(h.w.get_page(), Page::List);
        let catalog = h.catalog();
        let camera = &catalog.cameras()[0];
        assert_eq!(camera.name().as_str(), "Tapo C200");
        assert_eq!(camera.url().to_string(), "rtsp://192.168.1.60:554/stream1");
        assert_eq!(camera.substream(), None);
    }

    // ------------------------------------------------------------ consulta ONVIF

    fn streams(host: &str, name: Option<&str>) -> ResolvedStreams {
        let url = |path: &str| {
            RtspUrl::parse(&format!("rtsp://{host}:554{path}"))
                .unwrap()
                .url
        };
        ResolvedStreams {
            main: url("/stream1"),
            sub: (host != "192.168.1.60").then(|| url("/stream2")),
            suggested_name: name.map(str::to_string),
        }
    }

    /// Preenche o formulário de câmera nova e salva.
    fn save_new(h: &Harness, name: &str, address: &str, user: &str, password: &str) {
        add(&h.w, name, address, user, password);
    }

    #[test]
    fn cadastro_so_com_ip_grava_principal_e_secundario() {
        let h = harness(Box::new(MemoryCredentialStore::default()));
        h.resolver
            .returns(Ok(streams("192.168.1.164", Some("TP-Link C200"))));
        save_new(&h, "", "192.168.1.164", "admin", "segredo");
        assert_eq!(h.w.get_page(), Page::Form);
        assert_eq!(h.w.get_form_state(), FormState::LookingUp);
        assert_eq!(h.w.get_cameras().row_count(), 0, "nada é salvo antes");
        assert!(h.resolver.calls.lock().unwrap().is_empty());

        // Enter de novo durante a consulta não inicia outra.
        h.w.invoke_save_form();
        assert_eq!(h.jobs.count(), 1);

        h.finish(0);
        let calls = h.resolver.calls.lock().unwrap().clone();
        assert_eq!(
            calls,
            [(
                StreamTarget::Address(DeviceAddress::parse("192.168.1.164").unwrap()),
                Some("admin".into()),
                true
            )]
        );
        assert_eq!(h.w.get_page(), Page::List);
        assert_eq!(h.w.get_form_password(), "", "a senha sai da tela");
        let catalog = h.catalog();
        let camera = &catalog.cameras()[0];
        assert_eq!(camera.name().as_str(), "TP-Link C200", "nome sugerido");
        assert_eq!(camera.url().to_string(), "rtsp://192.168.1.164:554/stream1");
        assert_eq!(
            camera.substream().map(|s| s.to_string()).as_deref(),
            Some("rtsp://192.168.1.164:554/stream2")
        );
        assert_eq!(camera.username(), Some("admin"));
        let id = camera.id().clone();
        drop(catalog);
        assert_eq!(
            h.catalog()
                .connection_request(&id, None)
                .unwrap()
                .url
                .expose(),
            "rtsp://admin:segredo@192.168.1.164:554/stream1",
            "a senha digitada vai para o cofre"
        );
    }

    #[test]
    fn secundario_de_outro_host_e_recusado_no_campo() {
        let h = harness(Box::new(MemoryCredentialStore::default()));
        let mut s = streams("192.168.1.164", None);
        s.sub = Some(RtspUrl::parse("rtsp://10.9.9.9/outro").unwrap().url);
        h.resolver.returns(Ok(s));
        save_new(&h, "A", "192.168.1.164", "admin", "x");
        h.finish(0);
        assert_eq!(h.w.get_page(), Page::Form);
        assert_eq!(h.w.get_form_state(), FormState::Editing);
        assert!(
            h.w.get_url_error().starts_with("Stream secundário:"),
            "{}",
            h.w.get_url_error()
        );
        assert_eq!(h.w.get_cameras().row_count(), 0);
    }

    #[test]
    fn nome_digitado_prevalece_sobre_o_sugerido() {
        let h = harness(Box::new(MemoryCredentialStore::default()));
        h.resolver
            .returns(Ok(streams("192.168.1.164", Some("TP-Link C200"))));
        save_new(&h, "Garagem", "192.168.1.164", "admin", "x");
        h.finish(0);
        assert_eq!(h.catalog().cameras()[0].name().as_str(), "Garagem");
    }

    #[test]
    fn autenticacao_recusada_nao_salva() {
        let h = harness(Box::new(MemoryCredentialStore::default()));
        h.resolver.returns(Err(ResolveError::AuthenticationFailed));
        save_new(&h, "", "192.168.1.164", "admin", "errada");
        h.finish(0);
        assert_eq!(h.w.get_page(), Page::Form);
        assert_eq!(h.w.get_form_state(), FormState::Editing);
        assert_eq!(h.w.get_form_error(), "Usuário ou senha inválidos.");
        assert_eq!(h.w.get_form_password(), "", "a senha sai da tela");
        assert_eq!(h.w.get_cameras().row_count(), 0);
    }

    #[test]
    fn sem_onvif_escolher_hikvision_monta_os_caminhos() {
        let h = harness(Box::new(MemoryCredentialStore::default()));
        h.resolver.returns(Err(ResolveError::NotOnvif));
        save_new(&h, "", "192.168.1.164", "admin", "x");
        h.finish(0);
        assert_eq!(h.w.get_form_state(), FormState::ChooseVendor);
        assert_eq!(
            h.w.get_vendor_reason(),
            "",
            "o texto da tela já diz o motivo"
        );
        let vendors: Vec<String> = h.w.get_vendors().iter().map(|v| v.to_string()).collect();
        assert_eq!(
            vendors,
            [
                "TP-Link Tapo",
                "Hikvision",
                "Dahua / Intelbras / Amcrest",
                "Reolink",
                "Uniview",
                "Axis"
            ]
        );
        assert_eq!(h.w.get_cameras().row_count(), 0);

        h.w.invoke_choose_vendor(1);
        assert_eq!(h.w.get_page(), Page::List);
        let catalog = h.catalog();
        let camera = &catalog.cameras()[0];
        assert_eq!(
            camera.url().to_string(),
            "rtsp://192.168.1.164:554/Streaming/Channels/101"
        );
        assert_eq!(
            camera.substream().map(|s| s.to_string()).as_deref(),
            Some("rtsp://192.168.1.164:554/Streaming/Channels/102")
        );
        assert_eq!(camera.name().as_str(), "Câmera 192.168.1.164");
    }

    #[test]
    fn cancelar_fabricante_volta_ao_formulario_sem_salvar() {
        let h = harness(Box::new(MemoryCredentialStore::default()));
        h.resolver
            .returns(Err(ResolveError::Unreachable("tempo esgotado".into())));
        save_new(&h, "", "192.168.1.164", "", "");
        h.finish(0);
        assert_eq!(h.w.get_form_state(), FormState::ChooseVendor);
        assert_eq!(
            h.w.get_vendor_reason(),
            "Motivo: A câmera não respondeu: tempo esgotado"
        );
        h.w.invoke_escape_pressed();
        assert_eq!(h.w.get_page(), Page::Form);
        assert_eq!(h.w.get_form_state(), FormState::Editing);
        h.w.invoke_choose_vendor(1);
        assert_eq!(
            h.w.get_cameras().row_count(),
            0,
            "escolha tardia é ignorada"
        );
    }

    #[test]
    fn cancelar_consulta_ignora_o_resultado() {
        let h = harness(Box::new(MemoryCredentialStore::default()));
        h.resolver.returns(Ok(streams("192.168.1.164", None)));
        save_new(&h, "", "192.168.1.164", "admin", "x");
        h.w.invoke_cancel_lookup();
        assert_eq!(h.w.get_form_state(), FormState::Editing);
        assert_eq!(h.w.get_page(), Page::Form);
        h.finish(0);
        assert_eq!(h.ui.len(), 0);
        assert_eq!(h.w.get_cameras().row_count(), 0);
        assert_eq!(h.w.get_page(), Page::Form);

        // Esc também cancela; resultado já agendado é descartado.
        save_new(&h, "", "192.168.1.164", "admin", "x");
        h.jobs.run(1);
        assert_eq!(h.ui.len(), 1);
        h.w.invoke_escape_pressed();
        assert_eq!(
            h.w.get_page(),
            Page::Form,
            "primeiro Esc só cancela a consulta"
        );
        h.ui.drain(&h.w);
        assert_eq!(h.w.get_cameras().row_count(), 0);
        assert_eq!(h.w.get_form_state(), FormState::Editing);
        h.w.invoke_escape_pressed();
        assert_eq!(h.w.get_page(), Page::List);
    }

    #[test]
    fn url_completa_cadastra_sem_consulta() {
        let h = harness(Box::new(MemoryCredentialStore::default()));
        save_new(&h, "A", "rtsp://admin:x@192.168.1.164:554/stream1", "", "");
        assert_eq!(h.w.get_page(), Page::List);
        assert_eq!(h.w.get_cameras().row_count(), 1);
        assert_eq!(h.jobs.count(), 0);
        assert!(h.resolver.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn editar_com_ip_sem_senha_pede_a_senha() {
        let h = harness(Box::new(MemoryCredentialStore::default()));
        add(&h.w, "A", "rtsp://h/s", "admin", "p1");
        h.w.invoke_edit_camera(h.w.get_cameras().row_data(0).unwrap().id);
        h.w.set_form_url("192.168.1.164".into());
        h.w.invoke_save_form();
        assert_eq!(
            h.w.get_form_error(),
            "Informe a senha para consultar a câmera."
        );
        assert_eq!(h.jobs.count(), 0);

        h.resolver.returns(Ok(streams("192.168.1.164", Some("X"))));
        h.w.set_form_password("p1".into());
        h.w.invoke_save_form();
        h.finish(0);
        assert_eq!(h.w.get_cameras().row_count(), 1, "edita, não duplica");
        let catalog = h.catalog();
        assert_eq!(catalog.cameras()[0].name().as_str(), "A");
        assert_eq!(
            catalog.cameras()[0].url().to_string(),
            "rtsp://192.168.1.164:554/stream1"
        );
    }
}
