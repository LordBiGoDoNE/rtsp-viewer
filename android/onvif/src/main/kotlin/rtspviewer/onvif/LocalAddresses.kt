package rtspviewer.onvif

import java.net.InetAddress

/**
 * Endereços IP literais e a regra de "rede local" usada pela busca e, no app,
 * pelo destino do ONVIF em HTTP (ADR 0005 e 0006).
 *
 * O parse é feito aqui, sem `InetAddress.getByName`: no Android, um texto que
 * não é IP literal vira consulta DNS, e entrada de rede não pode disparar DNS.
 */
object LocalAddresses {
    /** IPv4 (4 bytes) ou IPv6 (16 bytes) de um IP literal; `null` se não for um. */
    fun parseLiteral(text: String): ByteArray? = if (text.contains(':')) parseIpv6(text) else parseIpv4(text)

    /** IPv4 decimal com quatro partes, sem zeros à esquerda (como o Rust). */
    fun parseIpv4(text: String): ByteArray? {
        val parts = text.split('.')
        if (parts.size != 4) return null
        val out = ByteArray(4)
        for ((i, p) in parts.withIndex()) {
            if (p.isEmpty() || p.length > 3 || !p.all { it in '0'..'9' }) return null
            if (p.length > 1 && p[0] == '0') return null
            val v = p.toInt()
            if (v > 255) return null
            out[i] = v.toByte()
        }
        return out
    }

    /** IPv6 com `::` opcional e IPv4 embutido no fim; sem zona (`%`). */
    fun parseIpv6(text: String): ByteArray? {
        if (text.isEmpty() || text.count { it == ':' } < 2) return null
        val dbl = text.indexOf("::")
        if (dbl >= 0 && text.indexOf("::", dbl + 1) >= 0) return null
        val head = if (dbl >= 0) text.substring(0, dbl) else text
        val tail = if (dbl >= 0) text.substring(dbl + 2) else ""
        // IPv4 embutido só no fim do endereço.
        if (dbl >= 0 && head.contains('.')) return null
        val headGroups = groups(head) ?: return null
        val tailGroups = groups(tail) ?: return null
        val total = headGroups.size + tailGroups.size
        if (dbl < 0 && total != 8) return null
        if (dbl >= 0 && total > 7) return null
        val words = IntArray(8)
        headGroups.forEachIndexed { i, w -> words[i] = w }
        tailGroups.forEachIndexed { i, w -> words[8 - tailGroups.size + i] = w }
        return ByteArray(16) { i -> (words[i / 2] shr (if (i % 2 == 0) 8 else 0)).toByte() }
    }

    /** Grupos de 16 bits; o último pode ser IPv4 (vira dois grupos). */
    private fun groups(s: String): List<Int>? {
        if (s.isEmpty()) return emptyList()
        val parts = s.split(':')
        val out = mutableListOf<Int>()
        for ((i, p) in parts.withIndex()) {
            if (i == parts.lastIndex && p.contains('.')) {
                val v4 = parseIpv4(p) ?: return null
                out.add(((v4[0].toInt() and 0xFF) shl 8) or (v4[1].toInt() and 0xFF))
                out.add(((v4[2].toInt() and 0xFF) shl 8) or (v4[3].toInt() and 0xFF))
            } else {
                if (p.isEmpty() || p.length > 4 || !p.all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' }) {
                    return null
                }
                out.add(p.toInt(16))
            }
        }
        return out
    }

    /**
     * Rede privada (RFC 1918), link-local ou loopback; no IPv6, loopback,
     * link-local (`fe80::/10`) e local único (`fc00::/7`). É a mesma regra da
     * origem aceita na busca (design D4 da `descoberta-onvif`).
     */
    fun isLocal(bytes: ByteArray): Boolean = when (bytes.size) {
        4 -> isPrivateV4(bytes) || isLinkLocalV4(bytes) || u(bytes[0]) == 127
        16 -> {
            val first = (u(bytes[0]) shl 8) or u(bytes[1])
            val loopback = bytes.take(15).all { it.toInt() == 0 } && bytes[15].toInt() == 1
            loopback || (first and 0xffc0) == 0xfe80 || (first and 0xfe00) == 0xfc00
        }
        else -> false
    }

    fun isLocal(address: InetAddress): Boolean = isLocal(address.address)

    /** Só RFC 1918 (10/8, 172.16/12, 192.168/16). */
    fun isPrivateV4(b: ByteArray): Boolean = b.size == 4 && (
        u(b[0]) == 10 ||
            (u(b[0]) == 172 && u(b[1]) in 16..31) ||
            (u(b[0]) == 192 && u(b[1]) == 168)
        )

    private fun isLinkLocalV4(b: ByteArray): Boolean = u(b[0]) == 169 && u(b[1]) == 254

    private fun u(b: Byte): Int = b.toInt() and 0xFF
}
