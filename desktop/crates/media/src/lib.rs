//! Adaptador FFmpeg do rtsp-viewer: implementa a porta `StreamPlayer`.

mod hw;
mod player;

pub use player::{FfmpegPlayer, PlayerConfig};

/// Inicializa o FFmpeg uma vez por processo e reduz o log dele a erros.
/// Pode ser chamada de qualquer thread, quantas vezes for preciso.
pub fn init() -> Result<(), ffmpeg_next::Error> {
    static INIT: std::sync::OnceLock<Result<(), ffmpeg_next::Error>> = std::sync::OnceLock::new();
    *INIT.get_or_init(|| {
        ffmpeg_next::init()?;
        ffmpeg_next::log::set_level(ffmpeg_next::log::Level::Error);
        Ok(())
    })
}

/// Prepara a decodificação por hardware em segundo plano (veja
/// `hw::warm_up`). Chamar depois de `init`, fora da thread da UI.
pub fn warm_up_hardware() {
    hw::warm_up();
}

/// Versão da libavcodec carregada, no formato `maior.menor.micro`.
pub fn avcodec_version() -> String {
    let v = ffmpeg_next::codec::version();
    format!("{}.{}.{}", v >> 16, (v >> 8) & 0xff, v & 0xff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffmpeg_carrega_e_informa_versao() {
        init().expect("FFmpeg deve inicializar");
        let versao = avcodec_version();
        let maior: u32 = versao.split('.').next().unwrap().parse().unwrap();
        // libavcodec 60 = FFmpeg 6.x (Ubuntu 24.04); 62 = FFmpeg 8.x (Windows).
        assert!(maior >= 60, "libavcodec {versao} é antiga demais");
    }

    #[test]
    fn decodificadores_do_mvp_disponiveis() {
        init().unwrap();
        for id in [ffmpeg_next::codec::Id::H264, ffmpeg_next::codec::Id::HEVC] {
            assert!(
                ffmpeg_next::decoder::find(id).is_some(),
                "decodificador {id:?} ausente no FFmpeg"
            );
        }
    }
}
