package rtspviewer.onvif

import java.util.Base64
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import rtspviewer.core.domain.Password

/** Mesmos casos de `auth.rs` do desktop. */
class AuthTest {
    private fun creds(user: String, pass: String) = Credentials(user, Password(pass))

    @Test
    fun passwordDigestDoExemploDaNorma() {
        // Exemplo clássico do WS-Security UsernameToken Profile 1.0.
        val nonce = Base64.getDecoder().decode("LKqI6G/AikKCQrN0zqZFlg==")
        assertEquals("tuOSpGlFlIXsozq4HFNeeGeFLEI=", Auth.passwordDigest(nonce, "2010-09-16T07:50:45Z", "userpassword"))
    }

    @Test
    fun cabecalhoNaoContemASenhaEEscapaUsuario() {
        val h = Auth.wsSecurityHeader(creds("ad<m>in", "s3gr3do-unico"), ByteArray(16) { 1 }, 0)
        assertFalse(h.contains("s3gr3do-unico"))
        assertTrue(h.contains("<wsse:Username>ad&lt;m&gt;in</wsse:Username>"))
        assertTrue(h.contains("<wsu:Created>1970-01-01T00:00:00Z</wsu:Created>"))
        assertTrue(h.contains("#PasswordDigest"))
        assertFalse(h.contains("#PasswordText"))
        Xml.parse("<r xmlns:s=\"x\">$h</r>")
    }

    @Test
    fun credenciaisNaoMostramASenha() {
        assertFalse(creds("admin", "s3gr3do-unico").toString().contains("s3gr3do-unico"))
    }

    @Test
    fun datasIdaEVolta() {
        for (t in listOf(0L, 951_782_400L, 1_795_516_365L, 4_102_444_800L)) {
            val s = Auth.formatUtc(t)
            val p = s.substring(5, 19).split('-', 'T', ':').map { it.toInt() }
            assertEquals(t, Auth.utcToUnix(s.substring(0, 4).toLong(), p[0], p[1], p[2], p[3], p[4]), s)
        }
        assertEquals("2026-11-24T10:32:45Z", Auth.formatUtc(1_795_516_365))
        assertNull(Auth.utcToUnix(2026, 13, 1, 0, 0, 0))
        assertNull(Auth.utcToUnix(2026, 1, 1, 24, 0, 0))
    }

    @Test
    fun deslocamentoDeRelogio() {
        assertEquals(10_900, ClockOffset.between(10_800, 0).cameraTime(100))
    }

    @Test
    fun desafioDigestComQopEOpaque() {
        val c = DigestChallenge.parse(
            """Digest realm="IP Camera(C1234)", qop="auth,auth-int", nonce="abc,def", opaque="xyz", stale="FALSE"""",
        )
        assertEquals("IP Camera(C1234)", c.realm)
        assertEquals("abc,def", c.nonce)
        assertTrue(c.qopAuth)
        assertEquals("xyz", c.opaque)
    }

    @Test
    fun respostaDigestDaRfc2617() {
        // Exemplo da RFC 2617, seção 3.5 (GET; aqui o método é parâmetro).
        val c = DigestChallenge(
            realm = "testrealm@host.com",
            nonce = "dcd98b7102dd2f0e8b11d0f600bfb0c093",
            opaque = "5ccc069c403ebaf9f0171e9517f40e41",
            qopAuth = true,
        )
        val h = c.authorization(creds("Mufasa", "Circle Of Life"), "GET", "/dir/index.html", "0a4f113b")
        assertTrue(h.contains("""response="6629fae49393a05397450978507c4ef1""""), h)
        assertFalse(h.contains("Circle Of Life"))
    }

    @Test
    fun desafiosInvalidos() {
        assertEquals(
            DigestChallenge.NOT_DIGEST,
            assertFailsWith<DigestChallenge.ChallengeException> { DigestChallenge.parse("""Basic realm="x"""") }.message,
        )
        val alg = assertFailsWith<DigestChallenge.ChallengeException> {
            DigestChallenge.parse("""Digest realm="x", nonce="y", algorithm=SHA-256""")
        }
        assertTrue(alg.message!!.endsWith("SHA-256"))
        assertEquals(
            DigestChallenge.INCOMPLETE,
            assertFailsWith<DigestChallenge.ChallengeException> { DigestChallenge.parse("""Digest realm="x"""") }.message,
        )
    }

    @Test
    fun noncesAleatoriosDiferentes() {
        assertFalse(Auth.randomBytes(16).contentEquals(Auth.randomBytes(16)))
        assertEquals(16, Auth.randomBytes(16).size)
    }
}
