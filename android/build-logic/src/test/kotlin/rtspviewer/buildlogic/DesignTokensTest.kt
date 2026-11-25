package rtspviewer.buildlogic

import java.io.File
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.assertThrows

class DesignTokensTest {
    private val real: String = File("../../design/tokens.json").readText()

    private fun gen(json: String) = DesignTokens.generate(json, "x.theme")

    @Test
    fun `gera o tema com os tokens do projeto`() {
        val out = gen(real)
        assertTrue(out.contains("package x.theme"))
        assertTrue(out.contains("val background = Color(0xFF0E1014)"))
        assertTrue(out.contains("val textMuted = Color(0xFF9AA1AB)"))
        assertTrue(out.contains("val sizeBody = 14.sp"))
        assertTrue(out.contains("val weightBold = FontWeight(600)"))
        assertTrue(out.contains("val listItemHeight = 60.dp"))
    }

    @Test
    fun `cor com alfa vira ARGB`() {
        assertTrue(gen(real).contains("val overlay = Color(0xB3000000)"))
    }

    @Test
    fun `tokens da janela do desktop ficam de fora`() {
        assertTrue(!gen(real).contains("windowWidth"))
    }

    @Test
    fun `cor malformada indica o token`() {
        val e = assertThrows<TokenError> { gen(real.replace("\"#0E1014\"", "\"#0E10\"")) }
        assertEquals("cor inválida (use #RRGGBB ou #RRGGBBAA): color.background", e.message)
    }

    @Test
    fun `token ausente indica o token`() {
        val e = assertThrows<TokenError> { gen(real.replace("\"danger\": \"#E5534B\",", "")) }
        assertEquals("token ausente: color.danger", e.message)
    }

    @Test
    fun `numero invalido indica o token`() {
        val e = assertThrows<TokenError> { gen(real.replace("\"xs\": 4", "\"xs\": -4")) }
        assertEquals("número inválido (inteiro positivo): space.xs", e.message)
    }

    @Test
    fun `json invalido falha com mensagem clara`() {
        val e = assertThrows<TokenError> { gen("{") }
        assertTrue(e.message!!.startsWith("design/tokens.json inválido"))
    }

    @Test
    fun `camelCase dos nomes`() {
        assertEquals("surfaceRaised", DesignTokens.camel("surface-raised"))
        assertEquals("xl", DesignTokens.camel("xl"))
    }
}

class UiColorCheckTest {
    @Test
    fun `aceita cores do tema e transparente`() {
        val src = """
            val a = Tokens.Colors.text
            val b = Color.Transparent
            // Color(0xFF000000) em comentário não conta
        """.trimIndent()
        assertEquals(emptyList<String>(), UiColorCheck.violations("A.kt", src))
    }

    @Test
    fun `recusa cor literal e cor nomeada`() {
        val src = "val a = Color(0xFF112233)\nval b = Color.Red\n"
        assertEquals(
            listOf("A.kt:1: val a = Color(0xFF112233)", "A.kt:2: val b = Color.Red"),
            UiColorCheck.violations("A.kt", src),
        )
    }
}

class CleartextHttpCheckTest {
    @Test
    fun `aceita codigo sem cliente HTTP e URLs apenas como texto`() {
        val src = """
            val url = "http://192.168.0.10/onvif/device_service"
            // HttpURLConnection em comentário não conta
            /**
             * openConnection() em KDoc não conta
             */
        """.trimIndent()
        assertEquals(emptyList<String>(), CleartextHttpCheck.violations("A.kt", src))
    }

    @Test
    fun `recusa clientes HTTP fora do modulo onvif`() {
        val src = """
            import java.net.HttpURLConnection
            val c = URL(u).openConnection()
            val s = URL(u).openStream()
            import okhttp3.OkHttpClient
            import io.ktor.client.HttpClient
            import java.net.http.HttpClient
            val w = WebView(ctx)
        """.trimIndent()
        assertEquals(
            listOf(
                "A.kt:1: import java.net.HttpURLConnection",
                "A.kt:2: val c = URL(u).openConnection()",
                "A.kt:3: val s = URL(u).openStream()",
                "A.kt:4: import okhttp3.OkHttpClient",
                "A.kt:5: import io.ktor.client.HttpClient",
                "A.kt:6: import java.net.http.HttpClient",
                "A.kt:7: val w = WebView(ctx)",
            ),
            CleartextHttpCheck.violations("A.kt", src),
        )
    }
}
