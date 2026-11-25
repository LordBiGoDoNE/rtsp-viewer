package rtspviewer.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import rtspviewer.app.ui.RtspViewerApp

/** Única Activity do app (design D5). O boot não abre rede nem lê o cofre. */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent { RtspViewerApp() }
    }
}
