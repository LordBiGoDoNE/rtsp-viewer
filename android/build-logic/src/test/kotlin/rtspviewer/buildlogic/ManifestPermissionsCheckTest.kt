package rtspviewer.buildlogic

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test

class ManifestPermissionsCheckTest {
    private fun manifest(extra: String = "", allowBackup: String = "false") = """
        <?xml version="1.0" encoding="utf-8"?>
        <manifest xmlns:android="http://schemas.android.com/apk/res/android" package="x">
            <uses-permission android:name="android.permission.INTERNET" />
            <uses-permission android:name="android.permission.ACCESS_NETWORK_STATE" />
            <uses-permission android:name="android.permission.ACCESS_WIFI_STATE" />
            <uses-permission android:name="android.permission.CHANGE_WIFI_MULTICAST_STATE" />
            $extra
            <application android:allowBackup="$allowBackup" />
        </manifest>
    """.trimIndent()

    @Test
    fun `manifesto com as quatro permissões e sem backup passa`() {
        assertEquals(emptyList<String>(), ManifestPermissionsCheck.violations(manifest()))
    }

    @Test
    fun `permissão trazida por biblioteca é recusada`() {
        val out = ManifestPermissionsCheck.violations(
            manifest("""<uses-permission android:name="android.permission.WAKE_LOCK" />"""),
        )
        assertEquals(listOf("uses-permission fora da lista: android.permission.WAKE_LOCK"), out)
    }

    @Test
    fun `permissão própria declarada é recusada`() {
        val out = ManifestPermissionsCheck.violations(
            manifest(
                """<permission android:name="x.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION" android:protectionLevel="signature" />
                   <uses-permission android:name="x.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION" />""",
            ),
        )
        assertEquals(
            listOf(
                "uses-permission fora da lista: x.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION",
                "permission declarada: x.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION",
            ),
            out,
        )
    }

    @Test
    fun `uses-permission-sdk-23 também é conferida`() {
        val out = ManifestPermissionsCheck.violations(
            manifest("""<uses-permission-sdk-23 android:name="android.permission.CAMERA" />"""),
        )
        assertEquals(listOf("uses-permission-sdk-23 fora da lista: android.permission.CAMERA"), out)
    }

    @Test
    fun `backup ligado é recusado`() {
        assertEquals(
            listOf("android:allowBackup deve ser false (atual: 'true')"),
            ManifestPermissionsCheck.violations(manifest(allowBackup = "true")),
        )
    }
}
