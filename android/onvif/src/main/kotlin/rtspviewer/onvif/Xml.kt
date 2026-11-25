package rtspviewer.onvif

import java.io.StringReader
import javax.xml.XMLConstants
import javax.xml.parsers.ParserConfigurationException
import javax.xml.parsers.SAXParserFactory
import org.xml.sax.Attributes
import org.xml.sax.InputSource
import org.xml.sax.SAXException
import org.xml.sax.helpers.DefaultHandler

/**
 * Árvore XML mínima para respostas ONVIF (mesmo modelo de
 * `desktop/crates/onvif/src/xml.rs`).
 *
 * Firmwares usam prefixos de namespace diferentes (`tt:`, `trt:`, `ns2:`...);
 * por isso a árvore guarda só o nome local dos elementos e atributos, e as
 * buscas são por nome local. O tamanho da entrada é limitado por quem chama.
 */
class XmlElement internal constructor(val name: String) {
    internal val attrs = mutableListOf<Pair<String, String>>()
    internal val childList = mutableListOf<XmlElement>()
    internal val textBuilder = StringBuilder()

    val children: List<XmlElement> get() = childList

    val text: String get() = textBuilder.toString()

    fun textTrimmed(): String = text.trim()

    /** Primeiro filho direto com o nome local dado. */
    fun child(name: String): XmlElement? = children.firstOrNull { it.name == name }

    /** Caminho de filhos diretos, por nome local. */
    fun path(vararg names: String): XmlElement? {
        var el: XmlElement = this
        for (n in names) el = el.child(n) ?: return null
        return el
    }

    /** Primeiro descendente (busca em profundidade) com o nome local dado. */
    fun find(name: String): XmlElement? {
        for (c in children) {
            if (c.name == name) return c
            c.find(name)?.let { return it }
        }
        return null
    }

    /** Todos os descendentes com o nome local dado, em ordem de documento. */
    fun findAll(name: String): List<XmlElement> = mutableListOf<XmlElement>().also { collect(name, it) }

    private fun collect(name: String, out: MutableList<XmlElement>) {
        for (c in children) {
            if (c.name == name) out.add(c)
            c.collect(name, out)
        }
    }

    fun attr(name: String): String? = attrs.firstOrNull { it.first == name }?.second
}

class XmlException(message: String) : Exception("XML inválido: $message")

/**
 * Leitura com o SAX da plataforma (`javax.xml.parsers`, presente no JDK e no
 * Android; o StAX não existe no Android — ajuste do grupo 3 no design D4).
 *
 * Entrada de rede não confiável (ADR 0005): documento com DOCTYPE é recusado
 * antes do parse (sem DTD não há entidade declarada nem externa; só as
 * predefinidas e as numéricas), as features de entidade externa são
 * desligadas onde a plataforma as reconhece e a profundidade é limitada.
 */
object Xml {
    /** Profundidade máxima aceita, contra documentos patológicos. */
    const val MAX_DEPTH = 64

    /** Lê o documento inteiro e devolve o elemento raiz. @throws XmlException */
    fun parse(xml: String): XmlElement {
        // Busca sem diferenciar maiúsculas: recusar a mais é o lado seguro.
        if (xml.contains("<!DOCTYPE", ignoreCase = true)) throw XmlException("DOCTYPE não é aceito")
        val handler = TreeBuilder()
        try {
            newParser().parse(InputSource(StringReader(xml)), handler)
        } catch (e: SAXException) {
            throw XmlException(e.message ?: "documento malformado")
        } catch (e: java.io.IOException) {
            throw XmlException(e.message ?: "falha de leitura")
        } catch (e: ParserConfigurationException) {
            throw XmlException(e.message ?: "parser indisponível")
        }
        return handler.root ?: throw XmlException("documento vazio")
    }

    private fun newParser(): javax.xml.parsers.SAXParser {
        val f = SAXParserFactory.newInstance()
        // Nomes com prefixo são tratados pelo nome local, sem exigir que o
        // prefixo esteja declarado (como no desktop).
        f.isNamespaceAware = false
        f.isValidating = false
        bestEffort { f.setFeature(XMLConstants.FEATURE_SECURE_PROCESSING, true) }
        bestEffort { f.setFeature("http://apache.org/xml/features/disallow-doctype-decl", true) }
        bestEffort { f.setFeature("http://xml.org/sax/features/external-general-entities", false) }
        bestEffort { f.setFeature("http://xml.org/sax/features/external-parameter-entities", false) }
        bestEffort { f.setFeature("http://apache.org/xml/features/nonvalidating/load-external-dtd", false) }
        return f.newSAXParser()
    }

    /** Feature que a plataforma não reconhece (Android) não impede o parse: o DOCTYPE já foi recusado. */
    private inline fun bestEffort(block: () -> Unit) {
        try {
            block()
        } catch (_: Exception) {
            // ignorada de propósito
        }
    }

    /** Escapa texto para inclusão em conteúdo ou atributo XML. */
    fun escape(text: String): String = buildString(text.length) {
        for (c in text) {
            when (c) {
                '&' -> append("&amp;")
                '<' -> append("&lt;")
                '>' -> append("&gt;")
                '"' -> append("&quot;")
                '\'' -> append("&apos;")
                else -> append(c)
            }
        }
    }

    private fun localName(qName: String): String = qName.substringAfterLast(':')

    private class TreeBuilder : DefaultHandler() {
        val stack = ArrayDeque<XmlElement>()
        var root: XmlElement? = null

        override fun startElement(uri: String?, localName: String?, qName: String?, attributes: Attributes) {
            if (stack.size >= MAX_DEPTH) throw SAXException("profundidade excessiva")
            val raw = qName?.ifEmpty { null } ?: localName.orEmpty()
            val el = XmlElement(localName(raw))
            for (i in 0 until attributes.length) {
                val key = attributes.getQName(i)?.ifEmpty { null } ?: attributes.getLocalName(i).orEmpty()
                el.attrs.add(localName(key) to attributes.getValue(i).orEmpty())
            }
            stack.addLast(el)
        }

        override fun endElement(uri: String?, localName: String?, qName: String?) {
            val el = stack.removeLastOrNull() ?: throw SAXException("fechamento sem abertura")
            val parent = stack.lastOrNull()
            when {
                parent != null -> parent.childList.add(el)
                root == null -> root = el
                else -> throw SAXException("mais de um elemento raiz")
            }
        }

        override fun characters(ch: CharArray, start: Int, length: Int) {
            stack.lastOrNull()?.textBuilder?.appendRange(ch, start, start + length)
        }

        // Sem DTD não há entidade externa a resolver; qualquer pedido é recusado.
        override fun resolveEntity(publicId: String?, systemId: String?): InputSource =
            throw SAXException("entidade externa não é aceita")
    }
}
