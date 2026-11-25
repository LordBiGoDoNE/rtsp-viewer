package rtspviewer.core.application

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue
import rtspviewer.core.domain.Camera
import rtspviewer.core.domain.CameraId
import rtspviewer.core.domain.CameraName
import rtspviewer.core.domain.Password
import rtspviewer.core.domain.RtspUrl

/** Repositório e cofre em memória, com falhas simuladas. */
class Fake : CameraRepository, CredentialStore {
    var saved: List<Camera>? = null
    var failSave = false
    var initial: List<Camera> = emptyList()
    val secrets = mutableMapOf<CameraId, Password>()
    var failSecrets = false

    override fun load() = LoadedCameras(initial.map { it.copy() })

    override fun save(cameras: List<Camera>) {
        if (failSave) throw RepositoryException("disco cheio")
        saved = cameras.map { it.copy() }
    }

    override fun get(id: CameraId): Password? = secrets[id]

    override fun set(id: CameraId, password: Password) {
        if (failSecrets) throw CredentialException("cofre indisponível")
        secrets[id] = password
    }

    override fun delete(id: CameraId) {
        if (failSecrets) throw CredentialException("cofre indisponível")
        secrets.remove(id)
    }

    override val isPersistent = true
}

/** Mesmos casos de `catalog.rs` e `tests/catalog_substream.rs`. */
class CameraCatalogTest {
    private fun catalog(fake: Fake = Fake()): Pair<CameraCatalog, Fake> {
        val (c, w) = CameraCatalog.load(fake, fake)
        assertNull(w)
        return c to fake
    }

    private fun draft(name: String, url: String, username: String = "", password: PasswordInput = PasswordInput.Unchanged) =
        CameraDraft(name = name, url = url, username = username, password = password)

    @Test
    fun adicionaEPersiste() {
        val (c, fake) = catalog()
        val id = c.add(draft("Garagem", "rtsp://192.168.0.10:554/stream1"))
        val saved = fake.saved!!
        assertEquals(1, saved.size)
        assertEquals(id, saved[0].id)
        assertEquals("Garagem", saved[0].name.value)
        assertEquals(1, c.cameras.size)
    }

    @Test
    fun credenciaisDaUrlVaoParaOCofre() {
        val (c, fake) = catalog()
        val id = c.add(draft("Cam", "rtsp://admin:segredo@192.168.0.10/stream1"))
        val cam = c.get(id)!!
        assertEquals("rtsp://192.168.0.10/stream1", cam.url.toString())
        assertEquals("admin", cam.username)
        assertEquals("segredo", fake.secrets[id]!!.expose())
    }

    @Test
    fun validaTodosOsCamposDeUmaVez() {
        val (c, fake) = catalog()
        val err = assertFailsWith<CatalogError.Invalid> { c.add(draft("  ", "http://x")) }
        assertEquals(listOf(Field.Name, Field.Url), err.errors.map { it.field })
        assertNull(fake.saved)
    }

    @Test
    fun falhaAoGravarNaoAlteraALista() {
        val (c, fake) = catalog()
        fake.failSave = true
        assertFailsWith<CatalogError.Storage> {
            c.add(draft("A", "rtsp://h/s", "admin", PasswordInput.Set(Password("x"))))
        }
        assertTrue(c.cameras.isEmpty())
        assertTrue(fake.secrets.isEmpty(), "senha não deve ficar órfã")
    }

    @Test
    fun editaMantendoIdentidadeESenha() {
        val (c, fake) = catalog()
        val id = c.add(draft("A", "rtsp://h/s", "admin", PasswordInput.Set(Password("p1"))))
        c.edit(id, draft("B", "rtsp://h2/s", "admin"))
        assertEquals("B", c.get(id)!!.name.value)
        assertEquals("p1", fake.secrets[id]!!.expose())
    }

    @Test
    fun senhaVaziaOuClearApagaDoCofre() {
        val (c, fake) = catalog()
        val id = c.add(draft("A", "rtsp://u:p@h/s"))
        c.edit(id, draft("A", "rtsp://h/s", "u", PasswordInput.Set(Password(""))))
        assertFalse(fake.secrets.containsKey(id))
    }

    @Test
    fun editarOuRemoverInexistente() {
        val (c, _) = catalog()
        val id = CameraId.generate()
        assertFailsWith<CatalogError.NotFound> { c.edit(id, draft("A", "rtsp://h")) }
        assertFailsWith<CatalogError.NotFound> { c.remove(id) }
    }

    @Test
    fun removeCameraESenha() {
        val (c, fake) = catalog()
        val id = c.add(draft("A", "rtsp://u:p@h/s"))
        c.remove(id)
        assertTrue(c.cameras.isEmpty())
        assertTrue(fake.saved!!.isEmpty())
        assertTrue(fake.secrets.isEmpty())
    }

    @Test
    fun removerComCofreFalhandoAvisaMasRemove() {
        val (c, fake) = catalog()
        val id = c.add(draft("A", "rtsp://u:p@h/s"))
        fake.failSecrets = true
        assertFailsWith<CatalogError.Credentials> { c.remove(id) }
        assertTrue(c.cameras.isEmpty())
    }

    @Test
    fun pedidoDeConexaoIncluiCredenciaisSoNaUrlDeConexao() {
        val (c, _) = catalog()
        val id = c.add(draft("A", "rtsp://admin:s3gr3do@h:554/s"))
        val req = c.connectionRequest(id)
        assertEquals("rtsp://admin:s3gr3do@h:554/s", req.url.expose())
        assertEquals("rtsp://h:554/s", req.displayUrl)
        assertFalse(req.toString().contains("s3gr3do"))
    }

    @Test
    fun semSenhaNoCofrePedeAoUsuarioEGuarda() {
        val (c, fake) = catalog()
        val id = c.add(draft("A", "rtsp://h/s", "admin"))
        assertFailsWith<CatalogError.NeedsPassword> { c.connectionRequest(id) }
        assertEquals("rtsp://admin:x@h/s", c.connectionRequest(id, Password("x")).url.expose())
        assertTrue(fake.secrets.containsKey(id))
        c.connectionRequest(id)
    }

    @Test
    fun cameraSemUsuarioConectaSemSenha() {
        val (c, _) = catalog()
        val id = c.add(draft("A", "rtsp://h/s"))
        assertEquals("rtsp://h/s", c.connectionRequest(id).url.expose())
    }

    @Test
    fun falhaDoCofreAoAdicionarDevolveOIdDaCameraSalva() {
        val (c, fake) = catalog()
        fake.failSecrets = true
        val err = assertFailsWith<CatalogError.AddedWithoutPassword> {
            c.add(draft("A", "rtsp://h/s", "admin", PasswordInput.Set(Password("p1"))))
        }
        assertEquals(1, c.cameras.size)
        assertEquals(1, fake.saved!!.size)
        assertNotNull(c.get(err.id))
        fake.failSecrets = false
        c.edit(err.id, draft("A", "rtsp://h/s", "admin", PasswordInput.Set(Password("p1"))))
        assertEquals(1, c.cameras.size)
        assertEquals("p1", fake.secrets[err.id]!!.expose())
    }

    @Test
    fun editarRemovendoUsuarioApagaSenhaDoCofre() {
        val (c, fake) = catalog()
        val id = c.add(draft("A", "rtsp://u:p@h/s"))
        assertTrue(fake.secrets.containsKey(id))
        c.edit(id, draft("A", "rtsp://h/s"))
        assertNull(c.get(id)!!.username)
        assertFalse(fake.secrets.containsKey(id))
        assertEquals("rtsp://h/s", c.connectionRequest(id).url.expose())
    }

    @Test
    fun cameraSemUsuarioIgnoraSenhaQueRestouNoCofre() {
        val fake = Fake()
        val cam = Camera.create(CameraName.parse("X"), RtspUrl.parse("rtsp://h/s").url, null)
        fake.initial = listOf(cam)
        fake.secrets[cam.id] = Password("antiga")
        val (c, _) = catalog(fake)
        assertEquals("rtsp://h/s", c.connectionRequest(cam.id).url.expose())
    }

    @Test
    fun adicionarSemUsuarioNaoGuardaSenha() {
        val (c, fake) = catalog()
        val id = c.add(draft("A", "rtsp://h/s", password = PasswordInput.Set(Password("x"))))
        assertTrue(fake.secrets.isEmpty())
        assertEquals("rtsp://h/s", c.connectionRequest(id).url.expose())
    }

    @Test
    fun carregaListaExistente() {
        val fake = Fake()
        val cam = Camera.create(CameraName.parse("X"), RtspUrl.parse("rtsp://h").url, null)
        fake.initial = listOf(cam)
        val (c, _) = catalog(fake)
        assertEquals(listOf(cam), c.cameras)
    }

    @Test
    fun editarNaoAlteraAListaSeAGravacaoFalhar() {
        val (c, fake) = catalog()
        val id = c.add(draft("A", "rtsp://h/s"))
        fake.failSave = true
        assertFailsWith<CatalogError.Storage> { c.edit(id, draft("B", "rtsp://h/s")) }
        assertEquals("A", c.get(id)!!.name.value)
    }

    // ----------------------------------------------------------- stream secundário

    private fun subDraft(sub: SubstreamInput, url: String = "rtsp://192.168.1.164:554/stream1") =
        CameraDraft(name = "Portão", url = url, substream = sub)

    private fun comSecundario(): Triple<CameraCatalog, Fake, CameraId> {
        val (c, fake) = catalog()
        val id = c.add(subDraft(SubstreamInput.Set("rtsp://192.168.1.164:554/stream2")))
        return Triple(c, fake, id)
    }

    @Test
    fun cadastraComSecundarioSemCredenciais() {
        val (c, fake) = catalog()
        val id = c.add(subDraft(SubstreamInput.Set("rtsp://admin:segredo@192.168.1.164:554/stream2")))
        val cam = c.get(id)!!
        assertEquals("rtsp://192.168.1.164:554/stream2", cam.substream.toString())
        assertEquals(cam.substream, fake.saved!![0].substream)
    }

    @Test
    fun editarSemMexerMantemEClearRemove() {
        val (c, _) = catalog()
        val id = c.add(subDraft(SubstreamInput.Set("rtsp://192.168.1.164/stream2")))
        c.edit(id, subDraft(SubstreamInput.Unchanged))
        assertNotNull(c.get(id)!!.substream)
        c.edit(id, subDraft(SubstreamInput.Clear))
        assertNull(c.get(id)!!.substream)
    }

    @Test
    fun secundarioInvalidoEErroDeCampo() {
        val (c, fake) = catalog()
        val err = assertFailsWith<CatalogError.Invalid> { c.add(subDraft(SubstreamInput.Set("http://x"))) }
        assertTrue(err.errors[0].message.startsWith("Stream secundário:"), err.errors.toString())
        assertNull(fake.saved)
    }

    @Test
    fun cameraNovaSemSecundario() {
        val (c, _) = catalog()
        assertNull(c.get(c.add(subDraft(SubstreamInput.Unchanged)))!!.substream)
    }

    @Test
    fun trocarUrlNoMesmoHostSemMexerDescartaSecundario() {
        val (c, fake, id) = comSecundario()
        c.edit(id, subDraft(SubstreamInput.Unchanged, "rtsp://192.168.1.164:554/h264"))
        assertNull(c.get(id)!!.substream)
        assertNull(fake.saved!![0].substream)
    }

    @Test
    fun trocarHostSemMexerDescartaSecundario() {
        val (c, fake, id) = comSecundario()
        c.edit(id, subDraft(SubstreamInput.Unchanged, "rtsp://192.168.1.121:554/stream1"))
        assertNull(c.get(id)!!.substream)
        assertNull(fake.saved!![0].substream)
    }

    @Test
    fun trocarUrlComSetExplicitoGravaONovoSecundario() {
        val (c, _, id) = comSecundario()
        c.edit(id, subDraft(SubstreamInput.Set("rtsp://192.168.1.121:554/stream2"), "rtsp://192.168.1.121:554/stream1"))
        assertEquals("rtsp://192.168.1.121:554/stream2", c.get(id)!!.substream.toString())
    }

    @Test
    fun secundarioDeOutroHostEErroDeCampoENadaMuda() {
        val (c, fake, id) = comSecundario()
        val err = assertFailsWith<CatalogError.Invalid> {
            c.edit(id, subDraft(SubstreamInput.Set("rtsp://192.168.1.164:554/stream2"), "rtsp://192.168.1.121:554/stream1"))
        }
        assertTrue(err.errors[0].message.startsWith("Stream secundário:"))
        val cam = c.get(id)!!
        assertEquals("rtsp://192.168.1.164:554/stream1", cam.url.toString())
        assertNotNull(cam.substream)
        assertEquals("rtsp://192.168.1.164:554/stream1", fake.saved!![0].url.toString())

        val (c2, fake2) = catalog()
        assertFailsWith<CatalogError.Invalid> { c2.add(subDraft(SubstreamInput.Set("rtsp://10.0.0.5/stream2"))) }
        assertNull(fake2.saved)
    }
}
