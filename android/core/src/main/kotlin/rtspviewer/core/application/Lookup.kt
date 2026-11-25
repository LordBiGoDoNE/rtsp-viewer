package rtspviewer.core.application

import rtspviewer.core.domain.DeviceAddress
import rtspviewer.core.domain.Password
import rtspviewer.core.domain.RtspUrl
import rtspviewer.core.domain.Vendor
import rtspviewer.core.domain.asciiLowercase

/** O que o usuário digitou no campo de endereço. */
sealed interface AddressInput {
    /** URL RTSP completa: cadastro como antes, sem consulta. */
    data object Rtsp : AddressInput

    /** Só o endereço da câmera: consultar. */
    data class Device(val address: DeviceAddress) : AddressInput
}

/** Classifica o texto do campo de endereço. @throws rtspviewer.core.domain.InvalidAddressException */
fun classifyAddress(input: String): AddressInput {
    val t = input.trim()
    val lower = asciiLowercase(t)
    if (lower.startsWith("rtsp://") || lower.startsWith("rtsps://")) return AddressInput.Rtsp
    return AddressInput.Device(DeviceAddress.parse(t))
}

sealed interface LookupOutcome {
    data class Resolved(val streams: ResolvedStreams) : LookupOutcome

    /** A câmera não respondeu ao ONVIF; o usuário escolhe o fabricante. */
    data class NeedsVendor(val reason: ResolveError) : LookupOutcome

    /** Credenciais recusadas: não salvar e não oferecer fabricante. */
    data object AuthenticationFailed : LookupOutcome
}

/**
 * Caso de uso: consulta a câmera (spec `camera-discovery`). Falha de
 * autenticação interrompe; as demais levam à escolha de fabricante.
 */
fun lookup(resolver: StreamResolver, target: StreamTarget, username: String?, password: Password?): LookupOutcome {
    val user = username?.trim()?.ifEmpty { null }
    val pass = password?.takeIf { !it.isEmpty() && user != null }
    return try {
        LookupOutcome.Resolved(resolver.resolve(target, user, pass))
    } catch (_: ResolveError.AuthenticationFailed) {
        LookupOutcome.AuthenticationFailed
    } catch (e: ResolveError) {
        LookupOutcome.NeedsVendor(e)
    }
}

/** Streams montados pelo caminho conhecido do fabricante. */
fun vendorStreams(vendor: Vendor, address: DeviceAddress): Pair<RtspUrl, RtspUrl?> = vendor.streamUrls(address)
