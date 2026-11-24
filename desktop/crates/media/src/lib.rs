//! Adaptador FFmpeg do rtsp-viewer.

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
        ffmpeg_next::init().expect("FFmpeg deve inicializar");
        let versao = avcodec_version();
        let maior: u32 = versao.split('.').next().unwrap().parse().unwrap();
        // libavcodec 60 = FFmpeg 6.x (Ubuntu 24.04); 62 = FFmpeg 8.x (Windows).
        assert!(maior >= 60, "libavcodec {versao} é antiga demais");
    }

    #[test]
    fn decodificadores_do_mvp_disponiveis() {
        ffmpeg_next::init().unwrap();
        for id in [ffmpeg_next::codec::Id::H264, ffmpeg_next::codec::Id::HEVC] {
            assert!(
                ffmpeg_next::decoder::find(id).is_some(),
                "decodificador {id:?} ausente no FFmpeg"
            );
        }
    }
}
