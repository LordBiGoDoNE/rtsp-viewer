package rtspviewer.onvif

import java.net.Inet4Address
import java.net.InetAddress
import java.net.InetSocketAddress
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** Mesmos casos dos testes de unidade de `discovery.rs` do desktop. */
class DiscoveryRulesTest {
    private val id = "uuid:11111111-2222-3333-4444-555555555555"

    private fun reply(relates: String, address: String, xaddrs: String, scopes: String) =
        """<?xml version="1.0"?><SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope" xmlns:wsa="http://schemas.xmlsoap.org/ws/2004/08/addressing" xmlns:d="http://schemas.xmlsoap.org/ws/2005/04/discovery">
            <SOAP-ENV:Header><wsa:MessageID>uuid:x</wsa:MessageID><wsa:RelatesTo>$relates</wsa:RelatesTo></SOAP-ENV:Header>
            <SOAP-ENV:Body><d:ProbeMatches><d:ProbeMatch><wsa:EndpointReference><wsa:Address>$address</wsa:Address></wsa:EndpointReference>
            <d:Types>dn:NetworkVideoTransmitter</d:Types><d:Scopes>$scopes</d:Scopes><d:XAddrs>$xaddrs</d:XAddrs>
            <d:MetadataVersion>1</d:MetadataVersion></d:ProbeMatch></d:ProbeMatches></SOAP-ENV:Body></SOAP-ENV:Envelope>"""

    private fun ip(s: String): InetAddress = InetAddress.getByAddress(assertNotNull(LocalAddresses.parseLiteral(s), s))

    private fun ip4(s: String): Inet4Address = ip(s) as Inet4Address

    private val lan = ip("192.168.1.164")

    private fun parse(xml: String, from: InetAddress = lan, messageId: String = id) =
        Discovery.parseProbeMatch(xml.toByteArray(), messageId, from)

    @Test
    fun leProbeMatchTapo() {
        val d = assertNotNull(
            parse(
                reply(
                    id, "urn:uuid:aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee", "http://192.168.1.164:2020/onvif/device_service",
                    "onvif://www.onvif.org/name/TP-IPC onvif://www.onvif.org/hardware/C200 onvif://www.onvif.org/Profile/Streaming",
                ),
            ),
        )
        assertEquals("urn:uuid:aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee", d.endpoint)
        assertEquals("http://192.168.1.164:2020/onvif/device_service", d.deviceUrl)
        assertEquals("192.168.1.164", d.host)
        assertEquals("TP-IPC", d.name)
        assertEquals("C200", d.hardware)
    }

    @Test
    fun prefereXaddrDoIpDeOrigemESoHttp() {
        val r = reply(
            id, "urn:a",
            "https://192.168.1.164/onvif/device_service http://10.0.0.9/onvif/device_service http://[fe80::1]/x http://192.168.1.164/onvif/device_service",
            "",
        )
        assertEquals("http://192.168.1.164/onvif/device_service", parse(r)?.deviceUrl)
        // Resposta chegou por outra interface: o host é o anunciado.
        val outra = reply(id, "urn:a", "http://192.168.240.1:8099/onvif/device_service", "")
        assertEquals("192.168.240.1", parse(outra, ip("172.23.0.1"))?.host)
        assertNull(parse(reply(id, "urn:a", "https://192.168.1.164/onvif/device_service", "")))
    }

    @Test
    fun nomeDecodificadoEHardwareComoReserva() {
        val r = reply(
            id, "urn:a", "http://192.168.1.164/x",
            "onvif://www.onvif.org/name/Port%C3%A3o_Frente onvif://www.onvif.org/hardware/DS-2CD",
        )
        assertEquals("Portão Frente", parse(r)?.name)
        assertEquals("DS-2CD", parse(reply(id, "urn:a", "http://192.168.1.164/x", "onvif://www.onvif.org/hardware/DS-2CD"))?.name)
    }

    @Test
    fun descartaRespostasInvalidas() {
        val boa = reply(id, "urn:a", "http://192.168.1.164/x", "")
        assertNotNull(parse(boa))
        assertNull(parse(reply("uuid:outra", "urn:a", "http://192.168.1.164/x", "")))
        assertNull(Discovery.parseProbeMatch("<lixo".toByteArray(), id, lan))
        assertNull(Discovery.parseProbeMatch(byteArrayOf(0xff.toByte(), 0xfe.toByte()), id, lan))
        assertNull(parse(boa, ip("8.8.8.8")))
        assertNull(parse(boa + " ".repeat(Discovery.MAX_DATAGRAM)))
        assertNull(parse(reply(id, "urn:a", "", "")))
    }

    @Test
    fun xaddrSoComIpConfiavel() {
        val device = { xaddrs: String -> parse(reply(id, "urn:a", xaddrs, ""))?.let { it.host to it.deviceUrl } }
        // Destinos que levariam as credenciais para fora da LAN ou para um
        // host que a origem não controla: o ProbeMatch é descartado.
        for (bad in listOf(
            "http://8.8.8.8/onvif/device_service",
            "http://evil.example/onvif/device_service",
            "http://camera.lan/onvif/device_service",
            "http://192.168.1.164@evil.com/onvif/device_service",
            "http://admin:x@192.168.1.164/onvif/device_service",
            "http://[2001:db8::1]/onvif/device_service",
            "http://100.64.0.1/onvif/device_service",
        )) {
            assertNull(device(bad), bad)
        }
        // Os inválidos são pulados; vale o primeiro IP confiável.
        assertEquals("10.0.0.9" to "http://10.0.0.9/x", device("http://evil.example/x http://8.8.8.8/x http://10.0.0.9/x"))
        // Só IPv6 link-local: aceito.
        assertEquals("fe80::1" to "http://[fe80::1]:80/x", device("http://[fe80::1]:80/x"))
    }

    @Test
    fun limitaDispositivosDistintos() {
        val c = Discovery.DeviceCollector(id)
        for (i in 0 until Discovery.MAX_DEVICES + 50) {
            c.add(Discovery.Datagram(reply(id, "urn:d$i", "http://192.168.1.164/x", "").toByteArray(), lan))
        }
        // Um já visto depois do limite continua sem duplicar.
        c.add(Discovery.Datagram(reply(id, "urn:d0", "http://192.168.1.164/x", "").toByteArray(), lan))
        val list = c.result()
        assertEquals(Discovery.MAX_DEVICES, list.size)
        assertTrue(list.any { it.endpoint == "urn:d0" })
    }

    @Test
    fun ordenaPorIp() {
        val c = Discovery.DeviceCollector(id)
        for ((e, h) in listOf("urn:a" to "192.168.1.20", "urn:b" to "192.168.1.3", "urn:c" to "10.0.0.1")) {
            c.add(Discovery.Datagram(reply(id, e, "http://$h/x", "").toByteArray(), ip(h)))
        }
        assertEquals(listOf("10.0.0.1", "192.168.1.3", "192.168.1.20"), c.result().map { it.host })
    }

    @Test
    fun triagemAntesDaFila() {
        val boa = reply(id, "urn:a", "http://192.168.1.164/x", "")
        assertTrue(Discovery.worthQueueing(boa.toByteArray(), lan, id))
        assertFalse(Discovery.worthQueueing(reply("uuid:outra", "urn:a", "http://192.168.1.164/x", "").toByteArray(), lan, id))
        assertFalse(Discovery.worthQueueing(boa.toByteArray(), ip("8.8.8.8"), id))
        assertFalse(Discovery.worthQueueing(byteArrayOf(0xff.toByte(), 0xfe.toByte()), lan, id))
        assertFalse(Discovery.worthQueueing((boa + " ".repeat(Discovery.MAX_DATAGRAM)).toByteArray(), lan, id))
    }

    /**
     * Formato de câmeras TP-Link Tapo (prefixos `SOAP-ENV`/`wsa`/`wsdd`/`tdn`,
     * `MessageID` igual ao do Probe, `ReferenceProperties` vazio e `PortType`),
     * com valores fictícios.
     */
    @Test
    fun leProbeMatchNoFormatoDaTapo() {
        val r = """<?xml version="1.0" encoding="UTF-8"?>
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope" xmlns:SOAP-ENC="http://www.w3.org/2003/05/soap-encoding" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:wsa="http://schemas.xmlsoap.org/ws/2004/08/addressing" xmlns:wsdd="http://schemas.xmlsoap.org/ws/2005/04/discovery" xmlns:chan="http://schemas.microsoft.com/ws/2005/02/duplex" xmlns:wsa5="http://www.w3.org/2005/08/addressing" xmlns:tt="http://www.onvif.org/ver10/schema" xmlns:tds="http://www.onvif.org/ver10/device/wsdl" xmlns:tdn="http://www.onvif.org/ver10/network/wsdl"><SOAP-ENV:Header><wsa:MessageID>$id</wsa:MessageID><wsa:RelatesTo>$id</wsa:RelatesTo><wsa:To SOAP-ENV:mustUnderstand="true">urn:schemas-xmlsoap-org:ws:2005:04:discovery</wsa:To><wsa:Action SOAP-ENV:mustUnderstand="true">http://schemas.xmlsoap.org/ws/2005/04/discovery/ProbeMatches</wsa:Action></SOAP-ENV:Header><SOAP-ENV:Body><wsdd:ProbeMatches><wsdd:ProbeMatch><wsa:EndpointReference><wsa:Address>uuid:00000000-1111-4222-8333-444444444444</wsa:Address><wsa:ReferenceProperties></wsa:ReferenceProperties><wsa:PortType>ttl</wsa:PortType></wsa:EndpointReference><wsdd:Types>tdn:NetworkVideoTransmitter</wsdd:Types><wsdd:Scopes>onvif://www.onvif.org/name/C320WS onvif://www.onvif.org/hardware/C320WS onvif://www.onvif.org/Profile/Streaming onvif://www.onvif.org/location/Sala%201 onvif://www.onvif.org/type/NetworkVideoTransmitter</wsdd:Scopes><wsdd:XAddrs>http://10.20.30.40:2020/onvif/device_service</wsdd:XAddrs><wsdd:MetadataVersion>1</wsdd:MetadataVersion></wsdd:ProbeMatch></wsdd:ProbeMatches></SOAP-ENV:Body></SOAP-ENV:Envelope>
"""
        val from = ip("10.20.30.40")
        val d = assertNotNull(parse(r, from))
        assertEquals("uuid:00000000-1111-4222-8333-444444444444", d.endpoint)
        assertEquals("http://10.20.30.40:2020/onvif/device_service", d.deviceUrl)
        assertEquals("10.20.30.40", d.host)
        assertEquals("C320WS", d.name)
        assertEquals("C320WS", d.hardware)
        // Resposta de outra busca (MessageID/RelatesTo diferentes): descartada.
        assertNull(parse(r, from, "uuid:outra"))
    }

    private fun hosts(ip: String, prefix: Int) =
        Discovery.sweepHosts(ip4(ip).address, prefix).map { v ->
            "${v ushr 24}.${(v ushr 16) and 0xFF}.${(v ushr 8) and 0xFF}.${v and 0xFF}"
        }

    @Test
    fun varreduraDaSubRede24() {
        val h = hosts("192.168.50.7", 24)
        assertEquals(253, h.size)
        assertEquals("192.168.50.1", h.first())
        assertEquals("192.168.50.254", h.last())
        assertFalse("192.168.50.0" in h || "192.168.50.255" in h || "192.168.50.7" in h)
    }

    @Test
    fun varreduraDePrefixosMenoresQue24EmHosts() {
        // /25: só a metade de cima, sem rede, broadcast e o próprio endereço.
        val h = hosts("10.0.0.200", 25)
        assertEquals(125, h.size)
        assertEquals("10.0.0.129", h.first())
        assertEquals("10.0.0.254", h.last())
        // /30: um único vizinho.
        assertEquals(listOf("172.16.5.2"), hosts("172.16.5.1", 30))
        // /31 e /32: nada a varrer.
        assertTrue(hosts("10.0.0.1", 31).isEmpty())
        assertTrue(hosts("10.0.0.1", 32).isEmpty())
    }

    @Test
    fun varreduraDeRedeMaiorFicaNo24DoEndereco() {
        val h = hosts("172.20.3.7", 20)
        assertEquals(253, h.size)
        assertEquals("172.20.3.1", h.first())
        assertEquals("172.20.3.254", h.last())
        val h16 = hosts("192.168.7.10", 16)
        assertEquals(253, h16.size)
        assertTrue(h16.all { it.startsWith("192.168.7.") })
    }

    @Test
    fun varreduraSoEmEnderecoPrivado() {
        for (no in listOf("169.254.10.20", "127.0.0.1", "8.8.8.8", "100.64.0.1")) {
            assertTrue(hosts(no, 24).isEmpty(), no)
            assertTrue(hosts(no, 16).isEmpty(), no)
        }
    }

    @Test
    fun planoUnicastSemRepeticaoEComTeto() {
        // Duas interfaces na mesma sub-rede: a segunda não repete destinos, e
        // nenhuma varre os endereços da própria máquina.
        val plan = Discovery.unicastPlan(
            listOf(ip4("192.168.1.10") to 24, ip4("192.168.1.11") to 24, ip4("169.254.3.4") to 16, ip4("10.9.9.9") to null),
            null,
        )
        assertEquals(4, plan.size)
        assertEquals(252, plan[0].size)
        assertTrue(plan[0].all { it.port == 3702 })
        assertFalse(plan[0].any { it.address == ip("192.168.1.11") })
        assertTrue(plan[1].isEmpty())
        // Link-local e interface sem prefixo: nada.
        assertTrue(plan[2].isEmpty() && plan[3].isEmpty())
        // Muitas interfaces: no máximo MAX_UNICAST_TARGETS no total.
        val many = (0 until 8).map { ip4("10.0.$it.1") to 24 }
        val big = Discovery.unicastPlan(many, null)
        assertEquals(Discovery.MAX_UNICAST_TARGETS, big.sumOf { it.size })
        assertTrue(big.all { it.size <= 254 })
        assertTrue(big[7].isEmpty())
        // Destinos injetados (testes): os mesmos em cada interface.
        val alvo = InetSocketAddress(ip("127.0.0.1"), 9)
        assertEquals(listOf(listOf(alvo)), Discovery.unicastPlan(listOf(ip4("127.0.0.1") to null), listOf(alvo)))
    }

    @Test
    fun probeTemIdETipo() {
        val p = Discovery.probeMessage(id)
        assertTrue(p.contains("<a:MessageID>$id</a:MessageID>"))
        assertTrue(p.contains("dn:NetworkVideoTransmitter"))
        Xml.parse(p)
        val novo = Discovery.newMessageId()
        assertTrue(novo.startsWith("uuid:") && novo.length == 41, novo)
        assertFalse(novo == Discovery.newMessageId())
    }

    @Test
    fun nomeDoScopeLimitadoA60Caracteres() {
        assertEquals(60, Discovery.decodeScope("a".repeat(100)).length)
        assertEquals("100%", Discovery.decodeScope("100%"))
    }
}
