package rtspviewer.onvif

/** Envelopes SOAP 1.2 e leitura de respostas e falhas (`desktop/crates/onvif/src/soap.rs`). */
object Soap {
    const val SOAP_ENV = "http://www.w3.org/2003/05/soap-envelope"
    const val DEVICE_NS = "http://www.onvif.org/ver10/device/wsdl"
    const val MEDIA_NS = "http://www.onvif.org/ver10/media/wsdl"
    const val SCHEMA_NS = "http://www.onvif.org/ver10/schema"

    /** Limite de tamanho de uma resposta HTTP (design D1 da `descoberta-onvif`). */
    const val MAX_RESPONSE_BYTES = 256 * 1024

    fun envelope(securityHeader: String?, body: String): String =
        """<?xml version="1.0" encoding="UTF-8"?><s:Envelope xmlns:s="$SOAP_ENV"><s:Header>""" +
            securityHeader.orEmpty() + "</s:Header><s:Body>" + body + "</s:Body></s:Envelope>"

    /** Falha SOAP com os códigos (`Code/Value` e `Subcode/Value` aninhados) e o texto. */
    data class Fault(val codes: List<String>, val reason: String) {
        /** `ter:NotAuthorized` (ONVIF) ou texto equivalente de firmwares antigos. */
        fun isNotAuthorized(): Boolean {
            val r = reason.lowercase()
            return codes.any { it.substringAfterLast(':').trim().equals("NotAuthorized", ignoreCase = true) } ||
                r.contains("authoriz") ||
                r.contains("authority failure")
        }
    }

    sealed interface Reply {
        class Body(val element: XmlElement) : Reply

        class FaultReply(val fault: Fault) : Reply
    }

    /** Lê um envelope SOAP e devolve o `Body` (ou a falha); `null` se não for um envelope SOAP. */
    fun readReply(text: String): Reply? {
        val doc = try {
            Xml.parse(text)
        } catch (_: XmlException) {
            return null
        }
        if (doc.name != "Envelope") return null
        val body = doc.child("Body") ?: return null
        val f = body.child("Fault") ?: return Reply.Body(body)
        val codes = mutableListOf<String>()
        var code = f.child("Code")
        while (code != null) {
            code.child("Value")?.let { codes.add(it.textTrimmed()) }
            code = code.child("Subcode")
        }
        // SOAP 1.1 (alguns firmwares): faultcode / faultstring.
        f.child("faultcode")?.let { codes.add(it.textTrimmed()) }
        val reason = (f.find("Text") ?: f.child("faultstring"))?.textTrimmed().orEmpty()
        return Reply.FaultReply(Fault(codes, reason))
    }

    fun getSystemDateAndTime(): String = """<GetSystemDateAndTime xmlns="$DEVICE_NS"/>"""

    fun getDeviceInformation(): String = """<GetDeviceInformation xmlns="$DEVICE_NS"/>"""

    fun getCapabilitiesMedia(): String =
        """<GetCapabilities xmlns="$DEVICE_NS"><Category>Media</Category></GetCapabilities>"""

    fun getProfiles(): String = """<GetProfiles xmlns="$MEDIA_NS"/>"""

    fun getStreamUri(profileToken: String): String =
        """<GetStreamUri xmlns="$MEDIA_NS"><StreamSetup>""" +
            """<Stream xmlns="$SCHEMA_NS">RTP-Unicast</Stream>""" +
            """<Transport xmlns="$SCHEMA_NS"><Protocol>RTSP</Protocol></Transport>""" +
            "</StreamSetup><ProfileToken>${Xml.escape(profileToken)}</ProfileToken></GetStreamUri>"
}
