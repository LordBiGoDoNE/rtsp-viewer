//! Decodificação por hardware (D3D11VA no Windows, VAAPI no Linux).
//!
//! O `ffmpeg-next` não expõe `hw_device_ctx`; este módulo usa a API C
//! diretamente. Todo `unsafe` do crate fica aqui.
#![allow(unsafe_code)]

use std::ptr;
use std::sync::Mutex;

use ff::ffi::{
    AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX, AVBufferRef, AVHWDeviceType, AVPixelFormat,
    av_buffer_ref, av_buffer_unref, av_hwdevice_ctx_create, av_hwframe_transfer_data,
    avcodec_find_decoder, avcodec_get_hw_config,
};
use ffmpeg_next as ff;

/// Dispositivo de hardware ligado a um contexto de decodificação.
pub struct HwDevice {
    pub api: &'static str,
    pub pix_fmt: ff::format::Pixel,
    device: *mut AVBufferRef,
}

// SAFETY: o AVBufferRef é contado por referência e só é liberado no Drop; o
// dispositivo é usado pela thread de reprodução que o criou.
unsafe impl Send for HwDevice {}

impl Drop for HwDevice {
    fn drop(&mut self) {
        // SAFETY: `device` veio de av_hwdevice_ctx_create e é liberado uma vez.
        unsafe { av_buffer_unref(&mut self.device) };
    }
}

#[cfg(target_os = "windows")]
const CANDIDATES: &[(AVHWDeviceType, &str)] = &[
    (AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA, "d3d11va"),
    (AVHWDeviceType::AV_HWDEVICE_TYPE_DXVA2, "dxva2"),
];

#[cfg(target_os = "linux")]
const CANDIDATES: &[(AVHWDeviceType, &str)] = &[(AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI, "vaapi")];

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
const CANDIDATES: &[(AVHWDeviceType, &str)] = &[];

/// Dispositivo criado uma vez por processo e compartilhado pelos
/// decodificadores (criar um dispositivo D3D11 custa centenas de ms).
struct SharedDevice(*mut AVBufferRef);

// SAFETY: o contexto de dispositivo do FFmpeg é contado por referência com
// operações atômicas e serializa o acesso ao dispositivo internamente; o
// ponteiro em si nunca é liberado (vive até o fim do processo).
unsafe impl Send for SharedDevice {}
unsafe impl Sync for SharedDevice {}

static DEVICES: Mutex<Vec<(i32, Option<SharedDevice>)>> = Mutex::new(Vec::new());

/// Referência nova ao dispositivo do tipo pedido, criando-o na primeira vez.
/// Falhas também ficam em cache, para não repetir tentativas caras.
fn shared_device(device_type: AVHWDeviceType) -> Option<*mut AVBufferRef> {
    let mut devices = DEVICES.lock().unwrap_or_else(|e| e.into_inner());
    let key = device_type as i32;
    if !devices.iter().any(|(k, _)| *k == key) {
        let mut device: *mut AVBufferRef = ptr::null_mut();
        // SAFETY: chamada padrão de criação; `device` só é usado se r >= 0.
        let r = unsafe {
            av_hwdevice_ctx_create(&mut device, device_type, ptr::null(), ptr::null_mut(), 0)
        };
        let created = (r >= 0 && !device.is_null()).then_some(SharedDevice(device));
        devices.push((key, created));
    }
    let (_, shared) = devices.iter().find(|(k, _)| *k == key)?;
    // SAFETY: o buffer compartilhado é válido; av_buffer_ref cria uma
    // referência nova, liberada pelo HwDevice.
    shared.as_ref().map(|d| unsafe { av_buffer_ref(d.0) })
}

/// Cria antecipadamente os dispositivos de hardware desta plataforma, para
/// que a primeira câmera aberta não pague esse custo. Chamar fora da thread
/// da UI.
pub fn warm_up() {
    for &(device_type, _) in CANDIDATES {
        if let Some(mut r) = shared_device(device_type) {
            // SAFETY: referência criada acima, liberada uma vez.
            unsafe { av_buffer_unref(&mut r) };
            return;
        }
    }
}

/// Tenta ligar um dispositivo de hardware ao contexto antes de abrir o
/// decodificador. `None` significa decodificação por software.
pub fn attach(ctx: &mut ff::codec::context::Context, codec_id: ff::codec::Id) -> Option<HwDevice> {
    // SAFETY: ponteiros vindos do próprio FFmpeg; o contexto ainda não foi
    // aberto, então definir hw_device_ctx é permitido.
    unsafe {
        let codec = avcodec_find_decoder(codec_id.into());
        if codec.is_null() {
            return None;
        }
        for &(device_type, api) in CANDIDATES {
            let Some(pix_fmt) = hw_pix_fmt(codec, device_type) else {
                continue;
            };
            let Some(device) = shared_device(device_type) else {
                continue;
            };
            (*ctx.as_mut_ptr()).hw_device_ctx = av_buffer_ref(device);
            return Some(HwDevice {
                api,
                pix_fmt: pix_fmt.into(),
                device,
            });
        }
        None
    }
}

unsafe fn hw_pix_fmt(
    codec: *const ff::ffi::AVCodec,
    device_type: AVHWDeviceType,
) -> Option<AVPixelFormat> {
    let mut i = 0;
    loop {
        // SAFETY: avcodec_get_hw_config devolve null depois do último índice.
        let config = unsafe { avcodec_get_hw_config(codec, i) };
        if config.is_null() {
            return None;
        }
        // SAFETY: config não é nulo e aponta para dado estático do FFmpeg.
        let c = unsafe { &*config };
        if c.device_type == device_type
            && (c.methods & AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX as i32) != 0
        {
            return Some(c.pix_fmt);
        }
        i += 1;
    }
}

/// Copia um quadro da GPU para a memória (normalmente em NV12).
pub fn transfer(hw: &ff::frame::Video, sw: &mut ff::frame::Video) -> Result<(), ff::Error> {
    // SAFETY: os dois quadros são válidos; o FFmpeg aloca os buffers de `sw`.
    let r = unsafe { av_hwframe_transfer_data(sw.as_mut_ptr(), hw.as_ptr(), 0) };
    if r < 0 {
        Err(ff::Error::from(r))
    } else {
        Ok(())
    }
}
