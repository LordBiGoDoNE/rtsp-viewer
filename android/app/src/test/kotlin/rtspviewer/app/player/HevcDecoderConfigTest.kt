package rtspviewer.app.player

import androidx.media3.common.Format
import androidx.media3.common.MimeTypes
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertSame

class HevcDecoderConfigTest {
    private val vps = byteArrayOf(0, 0, 0, 1, 0x40, 0x01, 0x0c)
    private val sps = byteArrayOf(0, 0, 0, 1, 0x42, 0x01, 0x01)
    private val pps = byteArrayOf(0x44, 0x01, 0xc1.toByte())

    private fun format(mime: String, vararg csd: ByteArray) =
        Format.Builder().setSampleMimeType(mime).setInitializationData(csd.toList()).build()

    @Test
    fun hevcComVpsSpsPpsSeparadosViraUmCsd() {
        val f = withSingleHevcCsd(format(MimeTypes.VIDEO_H265, vps, sps, pps))
        assertEquals(1, f.initializationData.size)
        assertContentEquals(vps + sps + byteArrayOf(0, 0, 0, 1) + pps, f.initializationData[0])
        assertEquals(MimeTypes.VIDEO_H265, f.sampleMimeType)
    }

    @Test
    fun outrosFormatosNaoMudam() {
        val h264 = format(MimeTypes.VIDEO_H264, sps, pps)
        assertSame(h264, withSingleHevcCsd(h264))
        val hevcJunto = format(MimeTypes.VIDEO_H265, vps + sps + pps)
        assertSame(hevcJunto, withSingleHevcCsd(hevcJunto))
    }
}
