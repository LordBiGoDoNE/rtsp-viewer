package rtspviewer.onvif

import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

class LocalAddressesTest {
    private fun local(s: String) = LocalAddresses.isLocal(assertNotNull(LocalAddresses.parseLiteral(s), s))

    @Test
    fun origensConfiaveis() {
        for (ok in listOf("10.1.2.3", "172.16.0.1", "172.31.255.255", "192.168.0.1", "169.254.1.1", "127.0.0.1", "fe80::1", "fd00::5", "::1")) {
            assertTrue(local(ok), ok)
        }
        for (no in listOf("8.8.8.8", "100.64.0.1", "172.32.0.1", "0.0.0.0", "2001:db8::1", "::", "::ffff:192.168.1.1")) {
            assertFalse(local(no), no)
        }
    }

    @Test
    fun parseSoDeIpLiteral() {
        assertContentEquals(byteArrayOf(192.toByte(), 168.toByte(), 1, 5), LocalAddresses.parseLiteral("192.168.1.5"))
        val v6 = assertNotNull(LocalAddresses.parseLiteral("fe80::1"))
        assertTrue(v6.size == 16 && v6[0] == 0xfe.toByte() && v6[1] == 0x80.toByte() && v6[15] == 1.toByte())
        assertNotNull(LocalAddresses.parseLiteral("::ffff:10.0.0.1"))
        assertNotNull(LocalAddresses.parseLiteral("1:2:3:4:5:6:7:8"))
        for (bad in listOf(
            "", "camera.lan", "192.168.1", "192.168.1.256", "192.168.01.1", "1.2.3.4.5", "fe80::zz", "fe80:::1",
            "1::2::3", "1:2:3:4:5:6:7:8:9", "1:2:3:4:5:6:7", "fe80::1%wlan0", "1.2.3.4::", "[::1]",
        )) {
            assertNull(LocalAddresses.parseLiteral(bad), bad)
        }
    }
}
