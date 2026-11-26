import rtspviewer.buildlogic.GenerateDesignTokens

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
    id("rtspviewer.design-tokens")
}

// Assinatura do release: keystore do usuário fora do repositório (ADR 0006).
// Sem as variáveis, o release sai sem assinatura (como no CI).
val keystorePath: String? = System.getenv("RTSP_VIEWER_KEYSTORE")

android {
    namespace = "rtspviewer.app"
    // Compose 1.13 e Lifecycle 2.11 exigem compilar contra a API 37.1+; o
    // comportamento em tempo de execução continua o do targetSdk 36 (ADR 0006).
    compileSdk {
        version = release(37) { minorApiLevel = 2 }
    }

    defaultConfig {
        applicationId = "io.github.lordbigodone.rtspviewer"
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    signingConfigs {
        if (keystorePath != null) {
            create("release") {
                storeFile = file(keystorePath)
                storePassword = System.getenv("RTSP_VIEWER_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("RTSP_VIEWER_KEY_ALIAS") ?: "rtsp-viewer"
                keyPassword = System.getenv("RTSP_VIEWER_KEY_PASSWORD")
                    ?: System.getenv("RTSP_VIEWER_KEYSTORE_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            if (keystorePath != null) signingConfig = signingConfigs.getByName("release")
        }
        // Igual ao release, assinado com a chave de debug do SDK e com outro id:
        // usado nas medições e no teste de ponta a ponta do agente.
        create("e2e") {
            initWith(getByName("release"))
            applicationIdSuffix = ".e2e"
            signingConfig = signingConfigs.getByName("debug")
            matchingFallbacks += "release"
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = false
    }

    testOptions {
        // Classes do Media3 usam APIs do Android (relógio, Bundle) só em campos
        // auxiliares; nos testes JVM elas devolvem valores padrão.
        unitTests.isReturnDefaultValues = true
    }

    lint {
        warningsAsErrors = true
        abortOnError = true
        checkDependencies = true
    }

    packaging {
        resources.excludes += setOf("META-INF/{AL2.0,LGPL2.1}", "META-INF/*.version", "kotlin/**")
    }
}

androidComponents {
    onVariants { variant ->
        val tokens = tasks.named<GenerateDesignTokens>("generateDesignTokens")
        variant.sources.kotlin?.addGeneratedSourceDirectory(tokens, GenerateDesignTokens::outputDir)
    }
}

dependencies {
    implementation(project(":core"))
    implementation(project(":onvif"))

    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.material3)
    implementation(libs.activity.compose)
    implementation(libs.lifecycle.viewmodel.compose)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.core.ktx)
    implementation(libs.media3.exoplayer)
    implementation(libs.media3.exoplayer.rtsp)
    implementation(libs.coroutines.android)
    implementation(libs.serialization.json)

    testImplementation(platform(libs.junit.bom))
    testImplementation(libs.junit.jupiter)
    testImplementation(libs.kotlin.test)
    testImplementation(libs.kotlin.test.junit5)
    testImplementation(libs.coroutines.test)
    testRuntimeOnly(libs.junit.platform.launcher)

    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.compose.ui.test.junit4)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.ext.junit)
    debugImplementation(libs.compose.ui.test.manifest)
}

tasks.withType<Test>().configureEach {
    useJUnitPlatform()
}
