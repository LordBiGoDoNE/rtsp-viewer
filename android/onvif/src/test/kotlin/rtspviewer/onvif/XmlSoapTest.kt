package rtspviewer.onvif

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** Mesmos casos de `xml.rs` e `soap.rs` do desktop. */
class XmlSoapTest {
    @Test
    fun ignoraPrefixosEResolveEntidades() {
        val doc = Xml.parse(
            """<?xml version="1.0"?><s:Envelope xmlns:s="x"><s:Body><tds:Resp xmlns:tds="y">
               <tds:Model a:k="v&amp;w" xmlns:a="z">C200 &amp; &#65;</tds:Model></tds:Resp></s:Body></s:Envelope>""",
        )
        assertEquals("Envelope", doc.name)
        val model = assertNotNull(doc.find("Model"))
        assertEquals("C200 & A", model.textTrimmed())
        assertEquals("v&w", model.attr("k"))
        assertNotNull(doc.path("Body", "Resp", "Model"))
    }

    @Test
    fun prefixoNaoDeclaradoTambemEhLidoPeloNomeLocal() {
        assertEquals("Uri", Xml.parse("<tt:Uri>rtsp://h/s</tt:Uri>").name)
    }

    @Test
    fun recusaDoctypeEDocumentosQuebrados() {
        for (bad in listOf(
            "<!DOCTYPE x [<!ENTITY a 'b'>]><x/>",
            "<?xml version=\"1.0\"?><!doctype x SYSTEM \"file:///etc/passwd\"><x>&xxe;</x>",
            "<a><b></a>",
            "<a/><b/>",
            "",
            "<a>&desconhecida;</a>",
        )) {
            assertFailsWith<XmlException>(bad) { Xml.parse(bad) }
        }
    }

    @Test
    fun recusaProfundidadeExcessiva() {
        val xml = "<a>".repeat(100) + "</a>".repeat(100)
        assertFailsWith<XmlException> { Xml.parse(xml) }
        val ok = "<a>".repeat(Xml.MAX_DEPTH) + "</a>".repeat(Xml.MAX_DEPTH)
        Xml.parse(ok)
    }

    @Test
    fun escapaTexto() {
        assertEquals("a&amp;b&lt;c&gt;&quot;d&apos;", Xml.escape("""a&b<c>"d'"""))
    }

    @Test
    fun buscaTodosOsDescendentes() {
        val doc = Xml.parse("<r><p t='1'/><x><p t='2'/></x></r>")
        assertEquals(listOf("1", "2"), doc.findAll("p").map { it.attr("t") })
    }

    @Test
    fun cdataEComentariosSaoAceitos() {
        val doc = Xml.parse("<r><!-- c --><u><![CDATA[rtsp://h/a&b]]></u></r>")
        assertEquals("rtsp://h/a&b", doc.child("u")?.textTrimmed())
    }

    @Test
    fun envelopeComESemSeguranca() {
        val e = Soap.envelope(null, Soap.getProfiles())
        assertTrue(e.contains("<s:Header></s:Header>"))
        assertTrue(e.contains("""<GetProfiles xmlns="http://www.onvif.org/ver10/media/wsdl"/>"""))
        assertTrue(Soap.envelope("<x/>", "").contains("<s:Header><x/></s:Header>"))
        Xml.parse(Soap.envelope(null, Soap.getStreamUri("main")))
    }

    @Test
    fun tokenDoPerfilEscapado() {
        assertTrue(Soap.getStreamUri("a<b").contains("<ProfileToken>a&lt;b</ProfileToken>"))
    }

    @Test
    fun leFalhaNotAuthorizedSoap12() {
        val xml = """<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope" xmlns:ter="http://www.onvif.org/ver10/error">
          <env:Body><env:Fault><env:Code><env:Value>env:Sender</env:Value>
          <env:Subcode><env:Value>ter:NotAuthorized</env:Value></env:Subcode></env:Code>
          <env:Reason><env:Text xml:lang="en">Sender not Authorized</env:Text></env:Reason></env:Fault></env:Body></env:Envelope>"""
        val f = assertIs<Soap.Reply.FaultReply>(Soap.readReply(xml)).fault
        assertEquals(listOf("env:Sender", "ter:NotAuthorized"), f.codes)
        assertTrue(f.isNotAuthorized())
    }

    @Test
    fun leFalhaSoap11EOutrasFalhas() {
        val xml = """<s:Envelope xmlns:s="x"><s:Body><s:Fault><faultcode>s:Client</faultcode>
          <faultstring>The action requested requires authorization</faultstring></s:Fault></s:Body></s:Envelope>"""
        val f = assertIs<Soap.Reply.FaultReply>(Soap.readReply(xml)).fault
        assertTrue(f.isNotAuthorized())
        assertEquals("The action requested requires authorization", f.reason)
        assertFalse(Soap.Fault(listOf("env:Receiver"), "Action not supported").isNotAuthorized())
    }

    @Test
    fun naoEnvelope() {
        assertNull(Soap.readReply("<html><body>login</body></html>"))
        assertNull(Soap.readReply("lixo"))
    }
}
