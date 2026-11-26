package rtspviewer.buildlogic

import java.io.StringReader
import javax.xml.XMLConstants
import javax.xml.parsers.DocumentBuilderFactory
import org.w3c.dom.Element
import org.xml.sax.InputSource

/**
 * Verificação do manifesto mesclado do release (requisito "Permissões mínimas"
 * da spec android-distribution; ADR 0006): só as quatro permissões de rede,
 * nenhuma permissão própria declarada e backup desligado. Bibliotecas (Media3,
 * AndroidX) acrescentam permissões no merge; as que o app não usa são retiradas
 * com `tools:node="remove"` no manifesto do app, e esta verificação impede que
 * uma atualização de biblioteca traga outra sem revisão.
 * Devolve as violações encontradas, uma por linha.
 */
object ManifestPermissionsCheck {
    val ALLOWED = setOf(
        "android.permission.INTERNET",
        "android.permission.ACCESS_NETWORK_STATE",
        "android.permission.ACCESS_WIFI_STATE",
        "android.permission.CHANGE_WIFI_MULTICAST_STATE",
    )

    private const val ANDROID_NS = "http://schemas.android.com/apk/res/android"

    fun violations(manifestXml: String): List<String> {
        val factory = DocumentBuilderFactory.newInstance().apply {
            isNamespaceAware = true
            setFeature(XMLConstants.FEATURE_SECURE_PROCESSING, true)
            setFeature("http://apache.org/xml/features/disallow-doctype-decl", true)
        }
        val doc = factory.newDocumentBuilder().parse(InputSource(StringReader(manifestXml)))
        val out = mutableListOf<String>()

        fun elements(tag: String): List<Element> {
            val list = doc.getElementsByTagName(tag)
            return (0 until list.length).map { list.item(it) as Element }
        }
        fun Element.androidAttr(name: String): String = getAttributeNS(ANDROID_NS, name)

        for (tag in listOf("uses-permission", "uses-permission-sdk-23")) {
            for (e in elements(tag)) {
                val name = e.androidAttr("name")
                if (name !in ALLOWED) out += "$tag fora da lista: $name"
            }
        }
        for (e in elements("permission")) {
            out += "permission declarada: ${e.androidAttr("name")}"
        }
        val app = elements("application").firstOrNull()
        if (app == null) {
            out += "manifesto sem <application>"
        } else if (app.androidAttr("allowBackup") != "false") {
            out += "android:allowBackup deve ser false (atual: '${app.androidAttr("allowBackup")}')"
        }
        return out
    }
}
