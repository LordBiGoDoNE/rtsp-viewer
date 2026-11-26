package rtspviewer.core.domain

/**
 * Política de reconexão: espera crescente de 1, 2, 4, 8, 16 e 30 s (teto),
 * reiniciada depois de 10 s de reprodução estável.
 */
class ReconnectPolicy {
    private var attempt = 0

    /**
     * Registra uma falha e devolve o número da próxima tentativa (começando em
     * 1) e quanto esperar antes dela, em milissegundos.
     */
    fun nextDelay(): Pair<Int, Long> {
        val idx = attempt.coerceAtMost(DELAYS_SECS.size - 1)
        if (attempt < Int.MAX_VALUE) attempt += 1
        return attempt to DELAYS_SECS[idx] * 1000
    }

    /** Zera a sequência se a reprodução ficou estável por [STABLE_PLAYBACK_MS]. */
    fun recordPlayback(stableForMs: Long) {
        if (stableForMs >= STABLE_PLAYBACK_MS) attempt = 0
    }

    companion object {
        private val DELAYS_SECS = longArrayOf(1, 2, 4, 8, 16, 30)

        /** Tempo de reprodução contínua que zera a sequência de esperas. */
        const val STABLE_PLAYBACK_MS = 10_000L
    }
}
