package rtspviewer.app

import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import java.io.File
import rtspviewer.app.infrastructure.FileCredentialStore
import rtspviewer.app.infrastructure.JsonCameraRepository
import rtspviewer.app.infrastructure.KeystoreSecretCipher
import rtspviewer.app.infrastructure.OnvifStreamResolver
import rtspviewer.app.infrastructure.androidCameraDiscovery
import rtspviewer.app.player.Media3Engine
import rtspviewer.app.ui.AppPorts
import rtspviewer.core.application.CameraDiscovery
import rtspviewer.core.application.DiscoveredCamera

/**
 * Wiring do app (design D5): repositório e cofre no armazenamento privado,
 * ONVIF, busca e player Media3. Nada aqui abre rede, lê arquivo ou toca no
 * Keystore: tudo acontece só quando a UI pede.
 */
object AppContainer {
    /**
     * Ponto de substituição para os testes de UI: trocar antes de abrir a
     * `MainActivity` (e restaurar depois).
     */
    @Volatile
    var portsFactory: (Context) -> AppPorts = { defaultPorts(it) }

    fun defaultPorts(context: Context, dir: File? = null): AppPorts {
        val app = context.applicationContext
        val storage = dir ?: app.filesDir
        val engine = Media3Engine(app)
        return AppPorts(
            repository = JsonCameraRepository(storage),
            credentials = FileCredentialStore(storage, KeystoreSecretCipher()),
            resolver = OnvifStreamResolver(),
            discovery = LazyDiscovery { androidCameraDiscovery(app) },
            engine = engine,
            attachSurface = engine::setSurfaceView,
            version = { versionName(app) },
        )
    }

    /** Cria o adaptador de busca (serviços do sistema) só na primeira busca. */
    private class LazyDiscovery(create: () -> CameraDiscovery) : CameraDiscovery {
        private val real by lazy(create)

        override fun discover(): List<DiscoveredCamera> = real.discover()
    }

    private fun versionName(context: Context): String = try {
        val pm = context.packageManager
        val info = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            pm.getPackageInfo(context.packageName, PackageManager.PackageInfoFlags.of(0))
        } else {
            @Suppress("DEPRECATION")
            pm.getPackageInfo(context.packageName, 0)
        }
        info.versionName.orEmpty()
    } catch (_: PackageManager.NameNotFoundException) {
        ""
    }
}
