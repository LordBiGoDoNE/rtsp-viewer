package rtspviewer.app.ui

import rtspviewer.app.player.VideoEngine
import rtspviewer.core.application.CameraDiscovery
import rtspviewer.core.application.CameraRepository
import rtspviewer.core.application.CredentialException
import rtspviewer.core.application.CredentialStore
import rtspviewer.core.application.DiscoveredCamera
import rtspviewer.core.application.LoadedCameras
import rtspviewer.core.application.RepositoryWarning
import rtspviewer.core.application.ResolvedStreams
import rtspviewer.core.application.StreamResolver
import rtspviewer.core.application.StreamTarget
import rtspviewer.core.domain.Camera
import rtspviewer.core.domain.CameraId
import rtspviewer.core.domain.CameraName
import rtspviewer.core.domain.ConnectionUrl
import rtspviewer.core.domain.Password
import rtspviewer.core.domain.RtspUrl

/** Portas falsas: nada de rede, arquivo nem Keystore. */
class MemoryRepository(initial: List<Camera> = emptyList(), private val warning: RepositoryWarning? = null) : CameraRepository {
    var saved: List<Camera> = initial
    var loads = 0

    override fun load(): LoadedCameras {
        loads++
        return LoadedCameras(saved.map { it.copy() }, warning)
    }

    override fun save(cameras: List<Camera>) {
        saved = cameras.map { it.copy() }
    }
}

class MemoryCredentials : CredentialStore {
    val passwords = mutableMapOf<CameraId, Password>()
    var calls = 0
    var failSet: String? = null

    override fun get(id: CameraId): Password? {
        calls++
        return passwords[id]
    }

    override fun set(id: CameraId, password: Password) {
        calls++
        failSet?.let { throw CredentialException(it) }
        passwords[id] = password
    }

    override fun delete(id: CameraId) {
        calls++
        passwords.remove(id)
    }

    override val isPersistent: Boolean = true
}

class FakeResolver : StreamResolver {
    class Call(val target: StreamTarget, val username: String?, val password: Password?)

    val calls = mutableListOf<Call>()
    var onCall: () -> Unit = {}
    var answer: () -> ResolvedStreams = { error("sem resposta configurada") }

    override fun resolve(target: StreamTarget, username: String?, password: Password?): ResolvedStreams {
        calls += Call(target, username, password)
        onCall()
        return answer()
    }
}

class FakeDiscovery : CameraDiscovery {
    var calls = 0
    var onCall: (Int) -> Unit = {}
    var answer: (Int) -> List<DiscoveredCamera> = { emptyList() }

    override fun discover(): List<DiscoveredCamera> {
        val n = ++calls
        onCall(n)
        return answer(n)
    }
}

class FakeEngine : VideoEngine {
    val opened = mutableListOf<ConnectionUrl>()
    var closes = 0
    var listener: VideoEngine.Listener? = null

    override fun open(url: ConnectionUrl, listener: VideoEngine.Listener) {
        opened += url
        this.listener = listener
    }

    override fun close() {
        closes++
        listener = null
    }
}

fun camera(name: String, url: String, username: String? = null): Camera =
    Camera.restore(CameraId.generate(), CameraName.parse(name), RtspUrl.parse(url).url, username)

fun streams(host: String, name: String? = null) = ResolvedStreams(
    RtspUrl.parse("rtsp://$host:554/stream1").url,
    RtspUrl.parse("rtsp://$host:554/stream2").url,
    name,
)

fun found(host: String, name: String? = null) = DiscoveredCamera(host, "http://$host:2020/onvif/device_service", name)
