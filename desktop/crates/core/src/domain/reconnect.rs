use std::time::Duration;

/// Política de reconexão: espera crescente de 1, 2, 4, 8, 16 e 30 s (teto),
/// reiniciada depois de 10 s de reprodução estável.
#[derive(Debug, Clone)]
pub struct ReconnectPolicy {
    attempt: u32,
}

const DELAYS_SECS: [u64; 6] = [1, 2, 4, 8, 16, 30];

/// Tempo de reprodução contínua que zera a sequência de esperas.
pub const STABLE_PLAYBACK: Duration = Duration::from_secs(10);

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl ReconnectPolicy {
    pub fn new() -> Self {
        Self { attempt: 0 }
    }

    /// Registra uma falha e devolve quanto esperar antes da próxima tentativa,
    /// com o número dessa tentativa (começando em 1).
    pub fn next_delay(&mut self) -> (u32, Duration) {
        let idx = (self.attempt as usize).min(DELAYS_SECS.len() - 1);
        self.attempt = self.attempt.saturating_add(1);
        (self.attempt, Duration::from_secs(DELAYS_SECS[idx]))
    }

    /// Informa por quanto tempo a reprodução ficou estável; zera a sequência
    /// se passou de [`STABLE_PLAYBACK`].
    pub fn record_playback(&mut self, stable_for: Duration) {
        if stable_for >= STABLE_PLAYBACK {
            self.attempt = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn espera_cresce_ate_o_teto() {
        let mut p = ReconnectPolicy::new();
        let esperas: Vec<u64> = (0..8).map(|_| p.next_delay().1.as_secs()).collect();
        assert_eq!(esperas, [1, 2, 4, 8, 16, 30, 30, 30]);
    }

    #[test]
    fn numera_as_tentativas() {
        let mut p = ReconnectPolicy::new();
        assert_eq!(p.next_delay().0, 1);
        assert_eq!(p.next_delay().0, 2);
    }

    #[test]
    fn reproducao_estavel_reinicia_sequencia() {
        let mut p = ReconnectPolicy::new();
        p.next_delay();
        p.next_delay();
        p.record_playback(Duration::from_secs(9));
        assert_eq!(p.next_delay().1.as_secs(), 4);
        p.record_playback(Duration::from_secs(10));
        assert_eq!(p.next_delay(), (1, Duration::from_secs(1)));
    }
}
